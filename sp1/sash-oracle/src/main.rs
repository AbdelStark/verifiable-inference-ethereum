//! `sash-oracle DIR TAG`: runs upstream SASH's `verify_composed` on `DIR/TAG.{fixed,claims,proof}.json`
//! with no calibration, as the guest does, and prints one JSON object:
//! - `verdict`: the serialized `Verdict`, without its timing fields;
//! - `accepts`: `Verdict::accepts()`;
//! - `binding`: `Plan::new(&fixed.params).binding_json(fixed.layer)`, or `null` if `Fixed` does not parse.
//!
//! A panic prints `{"panic": "<message>"}` instead, still with exit code 0, so mutated bundles get a
//! deterministic outcome.

use std::panic::{self, AssertUnwindSafe};
use std::{env, fs, process};

use serde_json::{Value, json};
use zk_prover::compose::{Fixed, Plan, verify_composed};

const TIMING_FIELDS: [&str; 2] = ["verify_secs", "shape_secs"];

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: sash-oracle DIR TAG");
        process::exit(2);
    }
    let read = |kind: &str| {
        let path = format!("{}/{}.{kind}.json", args[1], args[2]);
        fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
    };
    let (fixed, claims, proof) = (read("fixed"), read("claims"), read("proof"));

    panic::set_hook(Box::new(|_| {}));
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        let verdict = verify_composed(&fixed, &claims, &proof);
        let accepts = verdict.accepts();
        let mut verdict = serde_json::to_value(&verdict).expect("Verdict serializes");
        if let Value::Object(map) = &mut verdict {
            for field in TIMING_FIELDS {
                map.remove(field);
            }
        }
        let binding = serde_json::from_slice::<Fixed>(&fixed)
            .ok()
            .map(|f| Plan::new(&f.params).binding_json(f.layer));
        json!({ "verdict": verdict, "accepts": accepts, "binding": binding })
    }));
    let out = outcome.unwrap_or_else(|payload| {
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "non-string panic payload".into());
        json!({ "panic": message })
    });
    println!("{out}");
}
