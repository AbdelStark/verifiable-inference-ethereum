// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {SashTypes} from "./SashTypes.sol";
import {ISP1VerifierWithHash} from "@sp1-contracts/ISP1Verifier.sol";
import {ISashSegmentVerifier} from "./ISashSegmentVerifier.sol";

/// @notice The SP1 v6 verifier getters the adapter checks at deployment: `ISP1VerifierWithHash` lacks `VK_ROOT`.
interface ISp1PlonkVerifierV6 is ISP1VerifierWithHash {
    /// @notice The recursion vk root the verifier requires in `proofBytes[36:68]`.
    // forge-lint: disable-next-line(mixed-case-function)
    function VK_ROOT() external pure returns (bytes32);
}

/// @title Sp1SashSegmentVerifier
/// @notice Production/testnet adapter verifying the BN254 PLONK proof of the M3 aggregator over the SASH segment
///         guests (`SPEC.md` §1), through the pinned SP1 v6.1.0 `SP1VerifierPlonk` and the pinned
///         `sashAggregatorVKey`.
/// @dev The constructor accepts only a verifier whose `VERIFIER_HASH()` is `SP1_PLONK_VERIFIER_HASH` and whose
///      `VK_ROOT()` is `SP1_VK_ROOT`, the v6.1.0 PLONK verifier's (`SECURITY.md` §6). `VK_ROOT` is checked too
///      because Sepolia's `V6_1_0_BROKEN_SP1_VERIFIER_PLONK` returns the pinned `VERIFIER_HASH` and differs only
///      in `VK_ROOT`. A contract without those getters, such as the `SP1VerifierGateway`, or an address without
///      code, reverts with empty returndata. The outer wrap relies on BN254 pairing hardness, the Aztec Ignition
///      universal SRS and SP1 v6 circuit soundness (`SECURITY.md` §2 and §6).
contract Sp1SashSegmentVerifier is ISashSegmentVerifier {
    /// @notice `VERIFIER_HASH()` of the v6.1.0 `SP1VerifierPlonk`; its first 4 bytes are the proof selector.
    bytes32 public constant SP1_PLONK_VERIFIER_HASH =
        0x5a093a2fcb46394f5cadfe55c44d4d572fad9cec7aeb38026b0278322ef07fac;
    /// @notice `VK_ROOT()` of the v6.1.0 `SP1VerifierPlonk`, the recursion vk root of sp1-sdk `6.10.0`'s proofs.
    bytes32 public constant SP1_VK_ROOT = 0x002f850ee998974d6cc00e50cd0814b098c05bfade466d28573240d057f25352;

    ISp1PlonkVerifierV6 public immutable SP1_VERIFIER;
    bytes32 public immutable SASH_AGGREGATOR_VKEY;

    error ZeroVerifierAddress();
    error ZeroProgramVKey();
    error WrongSp1Verifier(bytes32 verifierHash, bytes32 vkRoot);

    constructor(address sp1Verifier, bytes32 sashAggregatorVKey) {
        if (sp1Verifier == address(0)) revert ZeroVerifierAddress();
        if (sashAggregatorVKey == bytes32(0)) revert ZeroProgramVKey();
        ISp1PlonkVerifierV6 verifier = ISp1PlonkVerifierV6(sp1Verifier);
        bytes32 verifierHash = verifier.VERIFIER_HASH();
        bytes32 vkRoot = verifier.VK_ROOT();
        if (verifierHash != SP1_PLONK_VERIFIER_HASH || vkRoot != SP1_VK_ROOT) {
            revert WrongSp1Verifier(verifierHash, vkRoot);
        }
        SP1_VERIFIER = verifier;
        SASH_AGGREGATOR_VKEY = sashAggregatorVKey;
    }

    function verifierKind() external pure override returns (SashTypes.VerifierKind) {
        return SashTypes.VerifierKind.ZKVM_PLONK;
    }

    function verifyAggregatedSegments(bytes32 publicDigest, bytes calldata proofBytes) external view override {
        SP1_VERIFIER.verifyProof(SASH_AGGREGATOR_VKEY, abi.encodePacked(publicDigest), proofBytes);
    }
}
