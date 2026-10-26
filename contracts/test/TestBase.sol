// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.38;

interface Vm {
    function expectRevert(bytes calldata revertData) external;
    function expectRevert(bytes4 revertSelector) external;
    function chainId(uint256 newChainId) external;
    function prank(address msgSender) external;
    function startBroadcast() external;
    function stopBroadcast() external;
    function envAddress(string calldata name) external view returns (address);
    function envBytes32(string calldata name) external view returns (bytes32);
}

abstract contract TestBase {
    Vm internal constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    error AssertionFailed();
    error AssertEqUintFailed(uint256 left, uint256 right);
    error AssertEqBytes32Failed(bytes32 left, bytes32 right);

    function assertTrue(bool cond) internal pure {
        if (!cond) revert AssertionFailed();
    }

    function assertEq(uint256 a, uint256 b) internal pure {
        if (a != b) revert AssertEqUintFailed(a, b);
    }

    function assertEq(bytes32 a, bytes32 b) internal pure {
        if (a != b) revert AssertEqBytes32Failed(a, b);
    }
}
