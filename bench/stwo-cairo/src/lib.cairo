//! S-two unit-cost microbenchmarks (issue #39): `bench` selects one, `n` is the op count.
//! 0: `n` chained `blake2s_compress` calls.
//! 1: `n` chained QM31 fused mul-adds, `acc * b + c`.
//! 2: ingest `n` M31 values from the arguments (`values`, which must hold exactly `n`).
use core::blake::blake2s_compress;
use core::box::BoxTrait;
use core::qm31::{m31, qm31, qm31_const};

#[executable]
fn main(bench: felt252, n: felt252, values: Array<m31>) -> felt252 {
    if bench == 0 {
        assert(values.len() == 0, 'values must be empty');
        blake(n)
    } else if bench == 1 {
        assert(values.len() == 0, 'values must be empty');
        qm31_mul_add(n)
    } else {
        assert(bench == 2, 'unknown bench');
        assert(values.len().into() == n, 'values must hold n M31s');
        ingest(values)
    }
}

fn blake(n: felt252) -> felt252 {
    let msg = BoxTrait::new([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
    let mut state = BoxTrait::new(
        [
            0x6b08e647, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
            0x5be0cd19,
        ],
    );
    let mut byte_count: u32 = 0;
    let mut i = n;
    while i != 0 {
        byte_count = byte_count + 64;
        state = blake2s_compress(state, byte_count, msg);
        i -= 1;
    }
    let [s0, _, _, _, _, _, _, _] = state.unbox();
    s0.into()
}

fn qm31_mul_add(n: felt252) -> felt252 {
    let b: qm31 = qm31_const::<3, 5, 7, 11>();
    let c: qm31 = qm31_const::<13, 17, 19, 23>();
    let mut acc: qm31 = qm31_const::<1, 2, 3, 4>();
    let mut i = n;
    while i != 0 {
        acc = acc * b + c;
        i -= 1;
    }
    let [w0, _, _, _] = core::qm31::QM31Trait::unpack(acc);
    w0.into()
}

fn ingest(values: Array<m31>) -> felt252 {
    let mut sum: felt252 = 0;
    for v in values {
        sum += v.into();
    }
    sum
}
