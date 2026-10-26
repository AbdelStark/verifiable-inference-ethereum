// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {M31} from "../field/M31.sol";
import {QM31} from "../field/QM31.sol";
import {Blake2s} from "./Blake2s.sol";

/// @title Blake2sM31Channel
/// @notice Bit-exact Stwo `Blake2sM31Channel` (`IS_M31_OUTPUT = true`) and SASH challenge replay.
library Blake2sM31Channel {
    struct State {
        bytes32 digest;
        uint32 nDraws;
    }

    function init() internal pure returns (State memory) {
        return State(bytes32(0), 0);
    }

    function mixRoot(State memory ch, bytes32 root) internal pure {
        ch.digest = Blake2s.hashPairM31(ch.digest, root);
        ch.nDraws = 0;
    }

    function mixFelts(State memory ch, QM31.Elem[] memory felts) internal pure {
        uint256 n = felts.length;
        bytes memory buf = new bytes(32 + 16 * n);
        for (uint256 i = 0; i < 32; ++i) {
            buf[i] = ch.digest[i];
        }
        for (uint256 k = 0; k < n; ++k) {
            QM31.writeLE(buf, 32 + 16 * k, felts[k]);
        }
        ch.digest = Blake2s.hashM31(buf);
        ch.nDraws = 0;
    }

    function drawU32s(State memory ch) internal pure returns (uint32[8] memory) {
        bytes memory msgBuf = new bytes(37);
        for (uint256 i = 0; i < 32; ++i) {
            msgBuf[i] = ch.digest[i];
        }
        uint32 c = ch.nDraws;
        // forge-lint: disable-next-line(unsafe-typecast)
        msgBuf[32] = bytes1(uint8(c));
        // forge-lint: disable-next-line(unsafe-typecast)
        msgBuf[33] = bytes1(uint8(c >> 8));
        // forge-lint: disable-next-line(unsafe-typecast)
        msgBuf[34] = bytes1(uint8(c >> 16));
        // forge-lint: disable-next-line(unsafe-typecast)
        msgBuf[35] = bytes1(uint8(c >> 24));
        msgBuf[36] = 0x00;
        unchecked {
            ch.nDraws = c + 1;
        }
        // Matches Stwo `Blake2sChannelGeneric<true>::draw_u32s`: hashes via `Blake2sHasherGeneric<true>`.
        bytes32 h = Blake2s.hashM31(msgBuf);
        return Blake2s.extractWordsLE(h);
    }

    function drawBaseFelts(State memory ch) internal pure returns (uint32[8] memory out) {
        while (true) {
            uint32[8] memory w = drawU32s(ch);
            bool ok = true;
            for (uint256 i = 0; i < 8; ++i) {
                if (w[i] >= M31.TWO_P) {
                    ok = false;
                    break;
                }
            }
            if (ok) {
                for (uint256 i = 0; i < 8; ++i) {
                    out[i] = M31.reduceU32(w[i]);
                }
                return out;
            }
        }
    }

    /// @notice Consumes `ceil(4 * nFelts / 8)` batches of 8 base felts (exact Stwo `draw_secure_felts`).
    function drawSecureFelts(State memory ch, uint256 nFelts) internal pure returns (QM31.Elem[] memory out) {
        uint256 batches = (4 * nFelts + 7) / 8;
        // forge-lint: disable-next-line(divide-before-multiply)
        uint32[] memory base = new uint32[](8 * batches);
        for (uint256 b = 0; b < batches; ++b) {
            uint32[8] memory f = drawBaseFelts(ch);
            for (uint256 j = 0; j < 8; ++j) {
                base[8 * b + j] = f[j];
            }
        }
        out = new QM31.Elem[](nFelts);
        for (uint256 i = 0; i < nFelts; ++i) {
            out[i] = QM31.Elem(base[4 * i], base[4 * i + 1], base[4 * i + 2], base[4 * i + 3]);
        }
    }

    /// @notice Replays SASH's segment transcript up to `w_residual_stream` (`f[3]`).
    function deriveWResidualStream(bytes32 preRoot, bytes32 modelRoot, bytes32 traceRoot, uint16 relationDraws)
        internal
        pure
        returns (QM31.Elem memory)
    {
        State memory ch = init();
        QM31.Elem[] memory pcsCfg = new QM31.Elem[](2);
        pcsCfg[0] = QM31.Elem(0, 1, 100, 0);
        pcsCfg[1] = QM31.Elem(1, 0, 0, 0);
        mixFelts(ch, pcsCfg);

        mixRoot(ch, preRoot);
        mixRoot(ch, modelRoot);
        mixRoot(ch, traceRoot);

        // Each SASH relation (`LookupElements::draw` / `EmbRel::draw`) calls `draw_secure_felts(2)`,
        // which consumes exactly 1 batch of 8 base felts (`drawBaseFelts`).
        for (uint256 r = 0; r < relationDraws; ++r) {
            drawBaseFelts(ch);
        }

        QM31.Elem[] memory f = drawSecureFelts(ch, 5);
        return f[3];
    }
}
