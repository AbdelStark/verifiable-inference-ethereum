// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {Test} from "forge-std/Test.sol";
import {M31} from "../src/field/M31.sol";
import {QM31} from "../src/field/QM31.sol";
import {Blake2s} from "../src/crypto/Blake2s.sol";
import {Blake2sM31Channel} from "../src/crypto/Blake2sM31Channel.sol";
import {TinyLlamaFixture} from "../src/sash/TinyLlamaFixture.sol";
import {SashTypes} from "../src/sash/SashTypes.sol";

contract PrimitivesAndChannelTest is Test {
    function testBlake2sRfc7693AndStwoVectors() public pure {
        // Stwo `single_hash_test` vector for "a":
        assertEq(
            Blake2s.hash(bytes("a")),
            0x4a0d129873403037c2cd9b9048203687f6233fb6738956e0349bd4320fec3e90
        );
        // Empty input RFC 7693 BLAKE2s-256 vector:
        assertEq(
            Blake2s.hash(bytes("")),
            0x69217a3079908094e11121d042354a7c1f55b6482ca1a51e1b250dfd1ed0eef9
        );
    }

    function testM31AndQM31Inverses() public pure {
        assertEq(M31.reduceU32(0x7fffffff), 0);
        assertEq(M31.reduceU32(0x80000000), 1);
        assertEq(M31.reduceU32(0xfffffffe), 0);
        assertEq(M31.reduceU32(0xffffffff), 1);
        assertEq(M31.mul(123456789, M31.inv(123456789)), 1);

        QM31.Elem memory x = QM31.Elem(1259081484, 630063634, 1062868201, 1393098785);
        QM31.Elem memory prod = QM31.mul(x, QM31.inv(x));
        assertTrue(QM31.eq(prod, QM31.one()));
    }

    function testTinyLlamaFiatShamirWResidualStreamParity() public pure {
        SashTypes.SegmentClaim[] memory segs = TinyLlamaFixture.segments();

        QM31.Elem memory wB0 = Blake2sM31Channel.deriveWResidualStream(
            segs[0].preRoot,
            segs[0].modelRoot,
            segs[0].traceRoot,
            segs[0].relationDraws
        );
        assertTrue(QM31.eq(wB0, QM31.Elem(1259081484, 630063634, 1062868201, 1393098785)));

        QM31.Elem memory wHead = Blake2sM31Channel.deriveWResidualStream(
            segs[22].preRoot,
            segs[22].modelRoot,
            segs[22].traceRoot,
            segs[22].relationDraws
        );
        assertTrue(QM31.eq(wHead, QM31.Elem(1297757949, 994229370, 1478950871, 2099916732)));
    }
}
