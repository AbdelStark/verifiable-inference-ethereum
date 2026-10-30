//! The CUDA client the proving commands share: one per process, built, used and dropped inside a Tokio
//! runtime, and the `VmHWM` of the `sp1-gpu-server` it starts. Linux x86_64 with an NVIDIA GPU only.

use std::fs;
use std::time::{Duration, Instant};

use sp1_sdk::blocking::{CudaProver, ProverClient};

/// Builds the CUDA client, runs `f` with it and prints `sp1-gpu-server`'s `VmHWM` before the client
/// drops.
///
/// sp1-cuda's client and proving key spawn their cleanup on the current Tokio runtime when dropped
/// (sp1-cuda 6.10.0 `client.rs`, `pk.rs`), and sp1-sdk's blocking API keeps its own runtime to itself:
/// dropped outside a runtime context, they panic in the destructor and the process aborts. So the
/// client, and every key `f` sets up, live and drop inside this runtime's context. Dropping the client
/// also kills `sp1-gpu-server` (`kill_on_drop`); the shutdown drops whatever of the cleanup has not
/// run.
pub fn with_cuda_client<T>(f: impl FnOnce(&CudaProver) -> Result<T, String>) -> Result<T, String> {
    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    let out = {
        let _context = runtime.enter();
        // Building the client downloads `sp1-gpu-server` on first use and starts it: the warm-up,
        // timed apart from setup and proving.
        let t = Instant::now();
        let client = ProverClient::builder().cuda().build();
        println!(
            "client build time (server download and start): {:.1} s",
            t.elapsed().as_secs_f64()
        );
        let out = f(&client);
        // Before the client drops and kills `sp1-gpu-server`.
        match gpu_server_vm_hwm() {
            Some(hwm) => println!("sp1-gpu-server VmHWM: {hwm}"),
            None => println!("sp1-gpu-server VmHWM: not found"),
        }
        out
    };
    runtime.shutdown_timeout(Duration::from_secs(10));
    out
}

/// `VmHWM` of this process' `sp1-gpu-server` child (`/proc/<pid>/status`), which runs the prover:
/// this process' own RSS misses it. `None` without `/proc` or without such a child.
pub fn gpu_server_vm_hwm() -> Option<String> {
    let me = std::process::id().to_string();
    fs::read_dir("/proc").ok()?.flatten().find_map(|entry| {
        let status = fs::read_to_string(entry.path().join("status")).ok()?;
        let field = |name: &str| {
            status
                .lines()
                .find_map(|l| l.strip_prefix(name))
                .map(|v| v.trim().to_string())
        };
        (field("Name:")? == "sp1-gpu-server" && field("PPid:")? == me)
            .then(|| field("VmHWM:"))
            .flatten()
    })
}
