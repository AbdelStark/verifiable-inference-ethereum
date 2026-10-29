//! One `SPEC.md` §1.1 rule at a time, on statements and verdicts built here. Rules 1 to 3 run before
//! `Plan::new`, so their controls go through `check_segment` itself; rules 4, 5, 7 and 8 are checked
//! on a constructed binding table, dequant `Spec`, `Verdict` and statement, since `Params::default()`
//! is not a statement `Plan::new` accepts. The synthetic bundles (`tests/bundles.rs`) run rules 1 to
//! 6 and 8 through `check_segment`, and rule 7 through `check_verdict` on a bundle's own `Plan` and
//! `Verdict` with a seam opened, since no corpus bundle has an open seam.

use sash_evm_primitives::tinyllama_fixture::{TINYLLAMA_TOK_IN, TINYLLAMA_TOK_OUT};
use serde_json::json;
use zk_prover::bf16::Round;
use zk_prover::compose::{EDGES, Params};
use zk_prover::gadgets::dequant::Variant;

use super::*;

fn fixed() -> Fixed {
    Fixed {
        params: Params::default(),
        log_blowup: 1,
        n_queries: 100,
        pow_bits: 0,
        sizes: vec![vec![10, 11], vec![12]],
        pre_root: "11".repeat(32),
        model_root: "22".repeat(32),
        layer: Some(1),
        block: Some(0),
        model: Some("33".repeat(32)),
        tok_in: vec![],
        tok_out: vec![],
    }
}

/// `check_segment` on `fixed`, with claims and a proof that would not decode: rules 1 to 3
/// reject before either is read.
fn check(fixed: &Fixed) -> Result<SegmentPublicValues, Reject> {
    check_segment(&serde_json::to_vec(fixed).unwrap(), b"", b"")
}

#[test]
fn constants_match_upstream_and_stwo() {
    assert_eq!(P, stwo::core::fields::m31::P);
    assert_eq!(P, 2_147_483_647);
    assert_eq!(LOG_BLOWUP, zk_prover::util::LOG_BLOWUP);
    assert_eq!(N_QUERIES, zk_prover::util::N_QUERIES);
    assert_eq!(POW_BITS, zk_prover::util::POW_BITS);
    assert_eq!(KMAX_SOUND, 21);
}

#[test]
fn malformed_fixed() {
    for bytes in [&b"not json"[..], b"", b"{}", b"{\"params\": 1}"] {
        assert!(matches!(
            check_segment(bytes, b"", b""),
            Err(Reject::Malformed(_))
        ));
    }
}

#[test]
fn token_id_below_p_passes_rule_1() {
    let mut f = fixed();
    f.tok_in = vec![0, P - 1];
    f.tok_out = vec![P - 1];
    f.log_blowup = 2;
    // Past rule 1, rule 2 rejects.
    assert!(matches!(check(&f), Err(Reject::SecurityParams { .. })));
}

#[test]
fn token_id_out_of_field() {
    for id in [P, P + 3, u32::MAX] {
        let mut f = fixed();
        f.tok_in = vec![5, id];
        assert_eq!(
            check(&f),
            Err(Reject::TokenIdOutOfField {
                vector: "tok_in",
                index: 1,
                id
            })
        );
        let mut f = fixed();
        f.tok_out = vec![id];
        assert_eq!(
            check(&f),
            Err(Reject::TokenIdOutOfField {
                vector: "tok_out",
                index: 0,
                id
            })
        );
    }
}

/// TinyLlama's own ids, one raised by `P`: it aliases the original id in M31.
#[test]
fn tinyllama_token_raised_by_p() {
    let mut f = fixed();
    f.tok_in = TINYLLAMA_TOK_IN.to_vec();
    f.tok_out = TINYLLAMA_TOK_OUT.to_vec();
    f.log_blowup = 2;
    assert!(matches!(check(&f), Err(Reject::SecurityParams { .. })));

    let mut g = f.clone();
    g.tok_in[8] += P;
    assert_eq!(
        check(&g),
        Err(Reject::TokenIdOutOfField {
            vector: "tok_in",
            index: 8,
            id: TINYLLAMA_TOK_IN[8] + P
        })
    );
    let mut g = f;
    g.tok_out[31] += P;
    assert_eq!(
        check(&g),
        Err(Reject::TokenIdOutOfField {
            vector: "tok_out",
            index: 31,
            id: TINYLLAMA_TOK_OUT[31] + P
        })
    );
}

#[test]
fn security_params_drifted() {
    let drifts: [fn(&mut Fixed); 3] = [
        |f| f.log_blowup = 2,
        |f| f.n_queries = 99,
        |f| f.pow_bits = 10,
    ];
    for drift in drifts {
        let mut f = fixed();
        drift(&mut f);
        assert_eq!(
            check(&f),
            Err(Reject::SecurityParams {
                log_blowup: f.log_blowup,
                n_queries: f.n_queries,
                pow_bits: f.pow_bits
            })
        );
    }
}

#[test]
fn missing_layer() {
    for layer in [None, Some(65536), Some(usize::MAX)] {
        let mut f = fixed();
        f.layer = layer;
        assert_eq!(check(&f), Err(Reject::MissingLayer { layer }));
    }
    let mut f = fixed();
    f.layer = Some(65535);
    assert_eq!(segment_index(&f), Ok(65535));
}

fn binding(unbound: &[u8]) -> Vec<Edge> {
    (1..=EDGES.len() as u8)
        .map(|n| Edge {
            n,
            edge: EDGES[usize::from(n) - 1],
            state: if unbound.contains(&n) {
                EdgeState::Unbound
            } else {
                EdgeState::Bound
            },
            why: None,
        })
        .collect()
}

#[test]
fn unbound_edge() {
    let f = fixed();
    assert_eq!(edges_bound(&f, &binding(&[])), Ok(()));
    assert_eq!(
        edges_bound(&f, &binding(&[13, 15])),
        Err(Reject::UnboundEdge {
            edges: vec![13, 15],
            argmax_lax: false
        })
    );
}

/// The lax argmax rejects even where the binding table marks nothing unbound (a statement with
/// no argmax, whose table leaves edge 16 absent).
#[test]
fn unbound_edge_argmax_lax() {
    let mut f = fixed();
    f.params.prove_argmax_lax = true;
    assert_eq!(
        edges_bound(&f, &binding(&[])),
        Err(Reject::UnboundEdge {
            edges: vec![],
            argmax_lax: true
        })
    );
}

fn spec(variant: Variant, kmax: u32, q_bits: u32, rem_bits: u32) -> Spec {
    Spec {
        variant,
        round: Round::Away,
        kmax,
        per_row: 1,
        limb_bits: (16, 14),
        q_bits,
        rem_bits,
        fold_alpha: 0,
    }
}

#[test]
fn unsound_dequant_kmax() {
    let mut f = fixed();
    f.params.kmax = 21;
    assert_eq!(dequant_sound(f.params.kmax, &[]), Ok(()));
    f.params.kmax = 22;
    assert_eq!(
        dequant_sound(f.params.kmax, &[]),
        Err(Reject::UnsoundDequant {
            kmax: 22,
            mats: vec![]
        })
    );
}

/// An `int8` dequant reads `|acc| < 2^(q_bits + rem_bits - 1)`, which leaves the field from
/// `q_bits + rem_bits = 32` (`Spec::sound`), whatever `kmax`. `Plan::new` refuses such params
/// (`ao_q_bits + ao_rem_bits <= 30`, or `31` in the `denp2` form), so the `Spec` is built here.
#[test]
fn unsound_dequant_spec() {
    let mut f = fixed();
    f.params.kmax = 16;
    let sound = spec(Variant::Int8, 16, 7, 24);
    let edge = spec(Variant::Int8, 16, 7, 25);
    assert!(sound.sound() && !edge.sound());
    assert_eq!(
        dequant_sound(
            f.params.kmax,
            &[spec(Variant::Pow2, 16, 7, 18), sound.clone()]
        ),
        Ok(())
    );
    assert_eq!(
        dequant_sound(f.params.kmax, &[sound, edge]),
        Err(Reject::UnsoundDequant {
            kmax: 16,
            mats: vec![1]
        })
    );
}

fn accepting_verdict() -> Verdict {
    Verdict {
        stark_ok: true,
        root_ok: true,
        model_root_ok: true,
        shape_root_ok: None,
        shape_root: None,
        shape_secs: 0.0,
        sums_cancel: true,
        qk_ok: vec![true],
        den_ok: vec![true],
        av_ok: true,
        den_link_ok: true,
        freivalds_ok: vec![true],
        sizes_ok: true,
        residual_stream_ok: vec![true],
        deq_out_ok: vec![true],
        deq_out_open: vec![],
        head_binding_ok: true,
        embed_ok: true,
        embed_tok_ok: true,
        head_tok_ok: true,
        rq_in_ok: vec![true],
        rq_out_ok: vec![true],
        rq_scale_ok: vec![true],
        rq_scale_open: vec![],
        ao_q_open: false,
        ao_q_ok: true,
        nn_in_ok: vec![true],
        nn_out_ok: vec![true],
        nn_pos_ok: true,
        nn_q_ok: vec![true],
        nn_open: false,
        stream_in: None,
        stream_out: None,
        boundary_roots: vec![],
        boundary_sides: vec![],
        error: None,
    }
}

#[test]
fn not_accepted() {
    assert_eq!(verdict_rules(&accepting_verdict()), Ok(()));
    let mut v = accepting_verdict();
    v.root_ok = false;
    v.freivalds_ok = vec![true, false];
    assert_eq!(
        verdict_rules(&v),
        Err(Reject::NotAccepted {
            failed: vec!["freivalds_ok".into(), "root_ok".into()],
            error: None
        })
    );
    // Rule 6 before rule 7.
    v.nn_open = true;
    assert!(matches!(verdict_rules(&v), Err(Reject::NotAccepted { .. })));
}

#[test]
fn open_seam() {
    let opens: [fn(&mut Verdict); 4] = [
        |v| v.deq_out_open = vec!["q".into()],
        |v| v.rq_scale_open = vec!["qkv/v".into()],
        |v| v.ao_q_open = true,
        |v| v.nn_open = true,
    ];
    for open in opens {
        let mut v = accepting_verdict();
        open(&mut v);
        assert!(v.accepts(), "upstream's accepts() ignores open seams");
        assert_eq!(
            verdict_rules(&v),
            Err(Reject::OpenSeam {
                deq_out_open: v.deq_out_open.clone(),
                rq_scale_open: v.rq_scale_open.clone(),
                ao_q_open: v.ao_q_open,
                nn_open: v.nn_open
            })
        );
    }
}

#[test]
fn not_single_block() {
    assert_eq!(single_block(&fixed()), Ok(()));
    let changes: [fn(&mut Fixed); 4] = [
        |f| f.block = None,
        |f| f.block = Some(1),
        // Block 0 with a prefix: the synthetic corpus' attention bundles (`--ctx 128`).
        |f| f.params.c = 128,
        |f| f.params.c = 32,
    ];
    for change in changes {
        let mut f = fixed();
        change(&mut f);
        assert_eq!(
            single_block(&f),
            Err(Reject::NotSingleBlock {
                block: f.block,
                prefix: f.params.c
            })
        );
    }
}

#[test]
fn display_names_the_variant() {
    let r = Reject::MissingLayer { layer: None };
    assert!(r.to_string().starts_with("MissingLayer: "));
}

fn hash_of_json(v: &serde_json::Value) -> [u8; 32] {
    fixed_hash(&serde_json::from_value(v.clone()).unwrap())
}

#[test]
fn fixed_hash_ignores_encoding_unknown_fields_and_tokens() {
    let base = fixed();
    let h = fixed_hash(&base);
    let compact = serde_json::to_vec(&base).unwrap();
    let pretty = serde_json::to_vec_pretty(&base).unwrap();
    assert_ne!(compact, pretty);
    for bytes in [compact, pretty] {
        assert_eq!(fixed_hash(&serde_json::from_slice(&bytes).unwrap()), h);
    }
    let mut v = serde_json::to_value(&base).unwrap();
    v["not_a_field"] = json!(1);
    v["params"]["not_a_param"] = json!("x");
    assert_eq!(hash_of_json(&v), h);

    let mut f = base;
    f.tok_in = TINYLLAMA_TOK_IN.to_vec();
    f.tok_out = TINYLLAMA_TOK_OUT.to_vec();
    assert_eq!(fixed_hash(&f), h);
    // Over the re-serialized struct: sha256 of its compact JSON, tokens cleared.
    assert_eq!(
        h,
        <[u8; 32]>::from(Sha256::digest(serde_json::to_vec(&fixed()).unwrap()))
    );
}

#[test]
fn fixed_hash_covers_the_statement() {
    let h = fixed_hash(&fixed());
    // `layer_boundary` is the field `--unsound-skip-layer-boundary` clears: `Params` carries
    // no `unsound_*` field of its own.
    let changes: [fn(&mut Fixed); 9] = [
        |f| f.sizes[0][0] += 1,
        |f| f.layer = Some(2),
        |f| f.block = Some(1),
        |f| f.model = Some("44".repeat(32)),
        |f| f.pre_root = "55".repeat(32),
        |f| f.model_root = "66".repeat(32),
        |f| f.params.kmax += 1,
        |f| f.params.prove_argmax_lax = true,
        |f| f.params.layer_boundary = true,
    ];
    for (i, change) in changes.into_iter().enumerate() {
        let mut f = fixed();
        change(&mut f);
        assert_ne!(fixed_hash(&f), h, "change {i} left fixedHash as it was");
    }
}
