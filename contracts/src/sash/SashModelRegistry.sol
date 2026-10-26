// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

import {M31} from "../field/M31.sol";
import {SashTypes} from "./SashTypes.sol";

/// @title SashModelRegistry
/// @notice Enrolls immutable `(preRoot, modelRoot)` manifests and sequence parameters for SASH LLM profiles.
contract SashModelRegistry {
    address public immutable OWNER;
    mapping(bytes32 => SashTypes.ModelManifest) private _manifests;

    event ModelEnrolled(
        bytes32 indexed modelId, uint16 numSegments, uint16 seqLen, uint32 eosTokenId, bytes32 manifestRootsHash
    );
    event ModelActiveUpdated(bytes32 indexed modelId, bool active);

    // the deploy scripts pass msg.sender; OWNER only gates enrollment
    // forge-lint: disable-next-line(missing-zero-check)
    constructor(address initialOwner) {
        OWNER = initialOwner;
    }

    modifier onlyOwner() {
        if (msg.sender != OWNER) revert SashTypes.UnauthorizedOwner(msg.sender);
        _;
    }

    function enrollModel(
        bytes32 modelId,
        uint16 numSegments,
        uint16 seqLen,
        uint32 eosTokenId,
        uint16 b0RelDraws,
        uint16 headRelDraws,
        bytes32 manifestRootsHash
    ) external onlyOwner {
        if (_manifests[modelId].numSegments != 0) {
            revert SashTypes.ModelAlreadyEnrolled(modelId);
        }
        if (numSegments < 2 || seqLen == 0 || manifestRootsHash == bytes32(0)) {
            revert SashTypes.InvalidManifestParameters();
        }
        M31.checkCanonical(eosTokenId);

        _manifests[modelId] = SashTypes.ModelManifest({
            numSegments: numSegments,
            seqLen: seqLen,
            eosTokenId: eosTokenId,
            b0RelDraws: b0RelDraws,
            headRelDraws: headRelDraws,
            manifestRootsHash: manifestRootsHash,
            active: true
        });

        emit ModelEnrolled(modelId, numSegments, seqLen, eosTokenId, manifestRootsHash);
    }

    function setModelActive(bytes32 modelId, bool active) external onlyOwner {
        if (_manifests[modelId].numSegments == 0) {
            revert SashTypes.ModelNotActive(modelId);
        }
        _manifests[modelId].active = active;
        emit ModelActiveUpdated(modelId, active);
    }

    function getManifest(bytes32 modelId) external view returns (SashTypes.ModelManifest memory m) {
        m = _manifests[modelId];
        if (!m.active) revert SashTypes.ModelNotActive(modelId);
    }
}
