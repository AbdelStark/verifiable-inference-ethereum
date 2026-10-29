#!/usr/bin/env bash
# Mines the TinyLlama-1.1B dev-mode settlement (DeployDevAnvil.s.sol) on a local Osaka anvil node,
# checks the receipts, and prints their gas. ANVIL_PORT overrides the port.
# The settle transaction is floor-bound (EIP-7623), so its receipt and snapshot don't move with execution;
# its execution gas, the top call's gasUsed in trace_transaction, is checked against settle_execution_gas.
set -euo pipefail

cd "$(dirname "$0")/.."

port="${ANVIL_PORT:-8577}"
rpc="http://127.0.0.1:${port}"
# anvil's first dev account, unlocked on the node, so no key is needed
sender=0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
tx_gas_cap=16777216 # EIP-7825
run_id=0x058d7f0512ac5fa539c1c21941f2bde128e85cedb4106adcff30e601d41d6b28 # SPEC.md §1.4
broadcast=broadcast/DeployDevAnvil.s.sol/31337/run-latest.json
snapshot=snapshots/VerifiableInferenceSettlement.json
settle_execution_gas=117227 # docs/EVM-GAS-MODEL.md §4; update both for an intended change

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

for tool in anvil cast forge jq; do
    command -v "$tool" >/dev/null 2>&1 || fail "'$tool' not found in PATH"
done
if cast chain-id --rpc-url "$rpc" >/dev/null 2>&1; then
    fail "port $port already serves an RPC node; set ANVIL_PORT to a free port"
fi

anvil --hardfork osaka --enable-tx-gas-limit --port "$port" --quiet &
anvil_pid=$!
trap 'kill "$anvil_pid" 2>/dev/null || true; wait "$anvil_pid" 2>/dev/null || true' EXIT
for _ in $(seq 100); do
    cast chain-id --rpc-url "$rpc" >/dev/null 2>&1 && break
    kill -0 "$anvil_pid" 2>/dev/null || fail "anvil exited; is port $port free?"
    sleep 0.1
done
cast chain-id --rpc-url "$rpc" >/dev/null 2>&1 || fail "anvil did not answer on port $port"

rm -f "$broadcast"
forge script script/DeployDevAnvil.s.sol:DeployDevAnvil --rpc-url "$rpc" --broadcast --unlocked --sender "$sender" \
    --slow >/dev/null || fail "forge script failed"

[[ "$(jq '.transactions | length' "$broadcast")" == 5 ]] || fail "expected 5 transactions"

settlement=$(jq -r '.transactions[] | select(.contractName == "VerifiableInferenceSettlement"
    and .transactionType == "CREATE") | .contractAddress' "$broadcast")
event_sig=$(cast keccak "InferenceVerified(bytes32,bytes32,bytes32,bytes32,bytes32,uint8)")

echo "DeployDevAnvil.s.sol on anvil, hardfork $(cast rpc anvil_nodeInfo --rpc-url "$rpc" | jq -r .hardFork)," \
    "$(forge --version | head -n 1)"
printf '%-52s %10s %10s %14s\n' "Transaction" "gasUsed" "gasLimit" "calldata (B)"
settle_gas=""
settle_hash=""
for i in $(seq 0 4); do
    tx=$(jq -c ".transactions[$i]" "$broadcast")
    hash=$(jq -r .hash <<<"$tx")
    receipt=$(jq -c --arg h "$hash" '.receipts[] | select(.transactionHash == $h)' "$broadcast")
    [[ -n "$receipt" ]] || fail "no receipt for $hash"
    [[ "$(jq -r .status <<<"$receipt")" == 0x1 ]] || fail "transaction $hash reverted"

    gas_used=$(cast to-dec "$(jq -r .gasUsed <<<"$receipt")")
    gas_limit=$(cast to-dec "$(jq -r .transaction.gas <<<"$tx")")
    ((gas_limit <= tx_gas_cap)) || fail "transaction $hash has gas limit $gas_limit > $tx_gas_cap"
    input=$(cast tx "$hash" input --rpc-url "$rpc")
    calldata_bytes=$(((${#input} - 2) / 2))

    name=$(jq -r .contractName <<<"$tx")
    fn=$(jq -r '.function // empty' <<<"$tx")
    if [[ -n "$fn" ]]; then label="$name.${fn%%(*}"; else label="$name deployment"; fi
    if [[ "$fn" == verifyAndSettle* ]]; then
        settle_gas=$gas_used
        settle_hash=$hash
    fi
    printf '%-52s %10s %10s %14s\n' "$label" "$gas_used" "$gas_limit" "$calldata_bytes"
done

events=$(jq -c --arg a "$settlement" --arg s "$event_sig" \
    '[.receipts[].logs[] | select((.address | ascii_downcase) == ($a | ascii_downcase) and .topics[0] == $s)]' \
    "$broadcast")
[[ "$(jq length <<<"$events")" == 1 ]] || fail "expected exactly one InferenceVerified from $settlement"
[[ "$(jq -r '.[0].topics[1]' <<<"$events")" == "$run_id" ]] || fail "InferenceVerified carries the wrong runId"
# data = promptHash, answerHash, verifierKind; DEV_TRANSCRIPT_ONLY = 0
[[ "$(cast to-dec "0x$(jq -r '.[0].data' <<<"$events" | tail -c 65)")" == 0 ]] \
    || fail "InferenceVerified verifierKind is not DEV_TRANSCRIPT_ONLY"

[[ -n "$settle_gas" ]] || fail "no verifyAndSettle transaction"
snapshot_gas=$(jq -r .verifyAndSettle "$snapshot")
[[ "$settle_gas" == "$snapshot_gas" ]] \
    || fail "verifyAndSettle receipt gasUsed $settle_gas != snapshot $snapshot_gas ($snapshot)"
settle_exec=$(cast to-dec "$(cast rpc trace_transaction "$settle_hash" --rpc-url "$rpc" | jq -r '.[0].result.gasUsed')")
echo "verifyAndSettle execution (trace_transaction top call gasUsed): $settle_exec"
[[ "$settle_exec" == "$settle_execution_gas" ]] \
    || fail "verifyAndSettle execution gas $settle_exec != settle_execution_gas $settle_execution_gas"

echo "OK: 5 transactions mined; InferenceVerified(runId $run_id, DEV_TRANSCRIPT_ONLY);" \
    "verifyAndSettle gasUsed matches $snapshot and its execution gas matches settle_execution_gas"
