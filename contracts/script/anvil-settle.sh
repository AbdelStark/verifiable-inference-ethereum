#!/usr/bin/env bash
# Mines the two TinyLlama-1.1B settlements on one local Osaka anvil node, checks the receipts, and prints their
# gas with the EIP-7623 breakdown of each settle transaction. ANVIL_PORT overrides the port.
# - DeployDevAnvil.s.sol: the dev-mode stack (DevTranscriptSegmentVerifier, allowDevVerifier = true).
# - DeployAnvilSp1.s.sol: the production stack, the committed PLONK proof (contracts/fixtures/tinyllama.plonk.json)
#   through the v6.1.0 SP1VerifierPlonk, allowDevVerifier = false and no dev verifier deployed. Its settle receipt
#   must stay below the M3 exit gate, 600,000 gas.
# Each settle's receipt gas is checked against its snapshot, and its execution gas, the top call's gasUsed in
# trace_transaction, against the pins below: the dev settle is floor-bound (EIP-7623), so its receipt doesn't
# move with execution.
set -euo pipefail

cd "$(dirname "$0")/.."

port="${ANVIL_PORT:-8577}"
rpc="http://127.0.0.1:${port}"
# anvil's first dev account, unlocked on the node, so no key is needed
sender=0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
tx_gas_cap=16777216 # EIP-7825
exit_gate=600000    # M3: the SP1 settle's receipt gasUsed must be below it
run_id=0x058d7f0512ac5fa539c1c21941f2bde128e85cedb4106adcff30e601d41d6b28 # SPEC.md §1.4
snapshot=snapshots/VerifiableInferenceSettlement.json
# docs/EVM-GAS-MODEL.md §4 and §5; update both for an intended change
dev_settle_execution_gas=117227
sp1_settle_execution_gas=406702

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

event_sig=$(cast keccak "InferenceVerified(bytes32,bytes32,bytes32,bytes32,bytes32,uint8)")
echo "anvil hardfork $(cast rpc anvil_nodeInfo --rpc-url "$rpc" | jq -r .hardFork), $(forge --version | head -n 1)"

# settle_run SCRIPT TXS KIND KIND_NAME SNAPSHOT_KEY EXECUTION_GAS [GATE]
# Mines SCRIPT, expects TXS transactions, exactly one InferenceVerified with SPEC.md §1.4's runId and
# verifierKind KIND, the settle receipt equal to SNAPSHOT_KEY in $snapshot (and below GATE when given) and its
# execution gas equal to EXECUTION_GAS.
settle_run() {
    local script=$1 txs=$2 kind=$3 kind_name=$4 snapshot_key=$5 execution_gas=$6 gate=${7:-}
    local broadcast="broadcast/$script/31337/run-latest.json"

    rm -f "$broadcast"
    forge script "script/$script:${script%%.*}" --rpc-url "$rpc" --broadcast --unlocked --sender "$sender" \
        --slow >/dev/null || fail "forge script $script failed"
    [[ "$(jq '.transactions | length' "$broadcast")" == "$txs" ]] || fail "$script: expected $txs transactions"

    local settlement
    settlement=$(jq -r '.transactions[] | select(.contractName == "VerifiableInferenceSettlement"
        and .transactionType == "CREATE") | .contractAddress' "$broadcast")

    echo
    echo "$script"
    printf '%-52s %10s %10s %14s %8s %10s\n' "Transaction" "gasUsed" "gasLimit" "calldata (B)" "zero" "non-zero"
    local settle_gas="" settle_hash="" settle_zero="" settle_nonzero=""
    local i tx hash receipt gas_used gas_limit input calldata_bytes zero nonzero name fn label
    for i in $(seq 0 $((txs - 1))); do
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
        zero=$(fold -w2 <<<"${input:2}" | grep -c '^00$' || true)
        nonzero=$((calldata_bytes - zero))

        name=$(jq -r .contractName <<<"$tx")
        [[ "$name" != null ]] || fail "transaction $hash has no contractName"
        fn=$(jq -r '.function // empty' <<<"$tx")
        if [[ -n "$fn" ]]; then label="$name.${fn%%(*}"; else label="$name deployment"; fi
        if [[ "$fn" == verifyAndSettle* ]]; then
            settle_gas=$gas_used
            settle_hash=$hash
            settle_zero=$zero
            settle_nonzero=$nonzero
        fi
        printf '%-52s %10s %10s %14s %8s %10s\n' "$label" "$gas_used" "$gas_limit" "$calldata_bytes" "$zero" "$nonzero"
    done
    [[ -n "$settle_gas" ]] || fail "$script: no verifyAndSettle transaction"

    local events
    events=$(jq -c --arg a "$settlement" --arg s "$event_sig" \
        '[.receipts[].logs[] | select((.address | ascii_downcase) == ($a | ascii_downcase) and .topics[0] == $s)]' \
        "$broadcast")
    [[ "$(jq length <<<"$events")" == 1 ]] || fail "$script: expected exactly one InferenceVerified from $settlement"
    [[ "$(jq -r '.[0].topics[1]' <<<"$events")" == "$run_id" ]] || fail "$script: InferenceVerified carries the wrong runId"
    # data = promptHash, answerHash, verifierKind
    [[ "$(cast to-dec "0x$(jq -r '.[0].data' <<<"$events" | tail -c 65)")" == "$kind" ]] \
        || fail "$script: InferenceVerified verifierKind is not $kind_name ($kind)"

    # the production run reaches no dev verifier: none is deployed, and the settlement forbids one
    local allow_dev
    allow_dev=$(cast call "$settlement" "ALLOW_DEV_VERIFIER()(bool)" --rpc-url "$rpc")
    if ((kind == 0)); then
        [[ "$allow_dev" == true ]] || fail "$script: the dev settlement has allowDevVerifier = $allow_dev"
    else
        [[ "$allow_dev" == false ]] || fail "$script: allowDevVerifier = $allow_dev"
        [[ "$(jq '[.transactions[] | select(.contractName == "DevTranscriptSegmentVerifier")] | length' \
            "$broadcast")" == 0 ]] || fail "$script: deploys a DevTranscriptSegmentVerifier"
    fi

    # EIP-7623: gasUsed = max(21,000 + standard calldata + execution, 21,000 + floor)
    local execution standard floor total
    execution=$(cast to-dec "$(cast rpc trace_transaction "$settle_hash" --rpc-url "$rpc" | jq -r '.[0].result.gasUsed')")
    standard=$((4 * settle_zero + 16 * settle_nonzero))
    floor=$((10 * settle_zero + 40 * settle_nonzero))
    total=$((21000 + standard + execution))
    if ((21000 + floor > total)); then total=$((21000 + floor)); fi
    echo "verifyAndSettle under EIP-7623: $settle_zero zero, $settle_nonzero non-zero calldata bytes;" \
        "standard $standard, execution $execution (trace_transaction top call gasUsed)," \
        "intrinsic + standard + execution $((21000 + standard + execution)), intrinsic + floor $((21000 + floor))"
    [[ "$settle_gas" == "$total" ]] || fail "$script: verifyAndSettle gasUsed $settle_gas != EIP-7623 total $total"

    local snapshot_gas
    snapshot_gas=$(jq -r ".$snapshot_key" "$snapshot")
    [[ "$settle_gas" == "$snapshot_gas" ]] \
        || fail "$script: verifyAndSettle receipt gasUsed $settle_gas != snapshot $snapshot_key $snapshot_gas ($snapshot)"
    [[ "$execution" == "$execution_gas" ]] \
        || fail "$script: verifyAndSettle execution gas $execution != pinned $execution_gas"
    if [[ -n "$gate" ]]; then
        ((settle_gas < gate)) || fail "$script: verifyAndSettle gasUsed $settle_gas is not below $gate"
    fi
    echo "OK: $txs transactions mined; InferenceVerified(runId $run_id, $kind_name);" \
        "verifyAndSettle gasUsed $settle_gas matches $snapshot_key and its execution gas the pin${gate:+; below the exit gate $gate}"
}

# VerifierKind: DEV_TRANSCRIPT_ONLY = 0, ZKVM_PLONK = 1
settle_run DeployDevAnvil.s.sol 5 0 DEV_TRANSCRIPT_ONLY verifyAndSettle "$dev_settle_execution_gas"
settle_run DeployAnvilSp1.s.sol 6 1 ZKVM_PLONK verifyAndSettlePlonk "$sp1_settle_execution_gas" "$exit_gate"
