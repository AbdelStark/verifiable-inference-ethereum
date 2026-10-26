//! Clean-room RFC 7693 `BLAKE2s-256` and Stwo `Blake2sM31` (`reduce_to_m31`) primitives.

use crate::field::M31;

const IV: [u32; 8] = [
    0x6A09_E667,
    0xBB67_AE85,
    0x3C6E_F372,
    0xA54F_F53A,
    0x510E_527F,
    0x9B05_688C,
    0x1F83_D9AB,
    0x5BE0_CD19,
];

const SIGMA: [[usize; 16]; 10] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];

#[inline]
fn g(v: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize, x: u32, y: u32) {
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
    v[d] = (v[d] ^ v[a]).rotate_right(16);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(12);
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
    v[d] = (v[d] ^ v[a]).rotate_right(8);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(7);
}

fn compress(h: &mut [u32; 8], block: &[u8; 64], t: u64, is_last: bool) {
    let mut m = [0u32; 16];
    for i in 0..16 {
        m[i] = u32::from_le_bytes([
            block[4 * i],
            block[4 * i + 1],
            block[4 * i + 2],
            block[4 * i + 3],
        ]);
    }

    let mut v = [0u32; 16];
    v[..8].copy_from_slice(h);
    v[8..16].copy_from_slice(&IV);
    v[12] ^= t as u32;
    v[13] ^= (t >> 32) as u32;
    if is_last {
        v[14] = !v[14];
    }

    for s in &SIGMA {
        g(&mut v, 0, 4, 8, 12, m[s[0]], m[s[1]]);
        g(&mut v, 1, 5, 9, 13, m[s[2]], m[s[3]]);
        g(&mut v, 2, 6, 10, 14, m[s[4]], m[s[5]]);
        g(&mut v, 3, 7, 11, 15, m[s[6]], m[s[7]]);
        g(&mut v, 0, 5, 10, 15, m[s[8]], m[s[9]]);
        g(&mut v, 1, 6, 11, 12, m[s[10]], m[s[11]]);
        g(&mut v, 2, 7, 8, 13, m[s[12]], m[s[13]]);
        g(&mut v, 3, 4, 9, 14, m[s[14]], m[s[15]]);
    }

    for i in 0..8 {
        h[i] ^= v[i] ^ v[i + 8];
    }
}

/// Computes unkeyed 32-byte RFC 7693 `BLAKE2s-256`.
pub fn blake2s256(data: &[u8]) -> [u8; 32] {
    let mut h = IV;
    // Parameter block: digest_length = 32 (0x20), key_length = 0, fanout = 1, depth = 1.
    h[0] ^= 0x0101_0020;

    if data.is_empty() {
        compress(&mut h, &[0u8; 64], 0, true);
    } else {
        let mut offset = 0usize;
        while data.len() - offset > 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[offset..offset + 64]);
            offset += 64;
            compress(&mut h, &block, offset as u64, false);
        }
        let mut last = [0u8; 64];
        let rem = data.len() - offset;
        last[..rem].copy_from_slice(&data[offset..]);
        compress(&mut h, &last, data.len() as u64, true);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.into_iter().enumerate() {
        out[4 * i..4 * (i + 1)].copy_from_slice(&word.to_le_bytes());
    }
    out
}

/// Reduces each of the eight little-endian `u32` words modulo `P = 2^31 - 1`
/// (exact Stwo `reduce_to_m31`).
pub fn reduce_to_m31(raw: [u8; 32]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for i in 0..8 {
        let w = u32::from_le_bytes([raw[4 * i], raw[4 * i + 1], raw[4 * i + 2], raw[4 * i + 3]]);
        let r = M31::reduce_u32(w).0;
        out[4 * i..4 * (i + 1)].copy_from_slice(&r.to_le_bytes());
    }
    out
}

/// Computes `Blake2sM31Hasher::hash(data)` (`reduce_to_m31(blake2s256(data))`).
#[inline]
pub fn blake2s_m31(data: &[u8]) -> [u8; 32] {
    reduce_to_m31(blake2s256(data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use blake2::{Blake2s256 as RefBlake2s256, Digest};

    #[test]
    fn rfc7693_and_stwo_vectors_match_reference_crate() {
        // Stwo `single_hash_test` vector for b"a":
        assert_eq!(
            hex::encode(blake2s256(b"a")),
            "4a0d129873403037c2cd9b9048203687f6233fb6738956e0349bd4320fec3e90"
        );
        // Empty input RFC 7693 BLAKE2s-256 vector:
        assert_eq!(
            hex::encode(blake2s256(b"")),
            "69217a3079908094e11121d042354a7c1f55b6482ca1a51e1b250dfd1ed0eef9"
        );

        // Differential check across multiple block boundaries (0, 37, 63, 64, 65, 128, 300 bytes)
        for len in [0usize, 1, 37, 63, 64, 65, 127, 128, 129, 300] {
            let msg: Vec<u8> = (0..len)
                .map(|i| (i.wrapping_mul(131) ^ 0x5a) as u8)
                .collect();
            let got = blake2s256(&msg);
            let want: [u8; 32] = RefBlake2s256::digest(&msg).into();
            assert_eq!(got, want, "mismatch at length {len}");
        }
    }

    #[test]
    fn reduce_to_m31_handles_high_bit_and_2p_boundary() {
        let mut raw = [0u8; 32];
        raw[0..4].copy_from_slice(&0x7fff_ffffu32.to_le_bytes()); // P -> 0
        raw[4..8].copy_from_slice(&0x8000_0000u32.to_le_bytes()); // P + 1 -> 1
        raw[8..12].copy_from_slice(&0xffff_fffeu32.to_le_bytes()); // 2P -> 0
        raw[12..16].copy_from_slice(&0xffff_ffffu32.to_le_bytes()); // 2P + 1 -> 1

        let red = reduce_to_m31(raw);
        assert_eq!(u32::from_le_bytes(red[0..4].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(red[4..8].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(red[8..12].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(red[12..16].try_into().unwrap()), 1);
    }
}
