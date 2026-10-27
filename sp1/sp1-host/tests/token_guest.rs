//! Runs the token-hash segment guest in the SP1 executor.

use sash_evm_primitives::tinyllama_fixture::TINYLLAMA_TOK_IN;
use sp1_host::{CYCLE_LIMIT, SEGMENT_GUEST_ELF};
use sp1_sdk::blocking::{ProverClient, prelude::*};

/// `tokInHash` from SPEC.md §1.4.
const SPEC_TOK_IN_HASH: &str = "3e683ee8603b85a4f5407e1af5f20a437ddadc9c098acc6aac357d533d3f32cb";
const P: u32 = (1 << 31) - 1;

struct Run {
    exit_code: u64,
    public_values: Vec<u8>,
    stderr: String,
    cycles: u64,
    gas: u64,
}

fn run(ids: &[u32]) -> Run {
    let mut stdin = SP1Stdin::new();
    stdin.write_vec(ids.iter().flat_map(|id| id.to_le_bytes()).collect());
    let client = ProverClient::builder().light().build();
    let (stderr_tx, stderr_rx) = tokio::sync::watch::channel(String::new());
    let (public_values, report) = client
        .execute(SEGMENT_GUEST_ELF, stdin)
        .calculate_gas(true)
        .cycle_limit(CYCLE_LIMIT)
        .stderr(stderr_tx)
        .run()
        .expect("executor error");
    Run {
        exit_code: report.exit_code,
        public_values: public_values.to_vec(),
        stderr: stderr_rx.borrow().clone(),
        cycles: report.total_instruction_count(),
        gas: report.gas().expect("calculate_gas(true) reports PGU"),
    }
}

#[test]
fn commits_spec_token_hash_for_fixture_tok_in() {
    let run = run(&TINYLLAMA_TOK_IN);
    assert_eq!(run.exit_code, 0, "stderr: {}", run.stderr);
    assert_eq!(hex::encode(&run.public_values), SPEC_TOK_IN_HASH);
    println!(
        "token-hash guest on the fixture tokIn: {} cycles, {} PGU",
        run.cycles, run.gas
    );
}

#[test]
fn rejects_token_id_at_or_above_p() {
    let mut ids = TINYLLAMA_TOK_IN;
    ids[3] += P;
    let run = run(&ids);
    assert_eq!(run.exit_code, 1);
    assert!(run.public_values.is_empty());
    assert!(
        run.stderr
            .contains(&format!("TokenIdOutOfField: id {} at index 3", ids[3])),
        "stderr: {}",
        run.stderr
    );
}
