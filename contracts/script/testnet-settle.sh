#!/usr/bin/env bash
# The Hoodi (560048) deployment of DeployTestnet.s.sol: rehearsed on an anvil fork, recorded, and verified.
# It reads RPC_URL, default $HOODI_RPC_URL, never takes a key and never prints an RPC URL.
#
#   rehearse [DEPLOYER=<address>]  Forks RPC_URL at its latest block on a local Osaka anvil (ANVIL_PORT, default
#                                  8578), impersonates and funds DEPLOYER (default anvil's first account), runs
#                                  `forge clean && forge build --locked` and the five-transaction broadcast from it,
#                                  records it into a temp file, moves the rehearsal's broadcast/ and cache/
#                                  directories into that temp dir, and writes deployments/<chainid>.preflight.json
#                                  (gitignored): the commit, Foundry, RPC_URL's eth_config and the block it was read
#                                  at, the fork block, the deployer and the predicted addresses.
#   record [SETTLE_TX=<hash>]      Checks broadcast/DeployTestnet.s.sol/<chainid>/run-latest.json against RPC_URL
#                                  and the deployed code against HEAD's build (`forge build --locked`), waits for
#                                  the transactions to finalize, reproduces the settle's receipt under EIP-7623
#                                  and writes deployments/<chainid>.json (jq -S), folding in and deleting the
#                                  preflight file. Against an anvil node (a rehearsal) it writes OUT instead, never
#                                  in or under deployments/, and folds in no preflight file. SETTLE_TX records a
#                                  front-run exact copy of the settle as the settle, with the four deployments from
#                                  the broadcast and the broadcast's own settle reverted with RunAlreadySettled or
#                                  never sent.
#   verify                         Verifies the three recorded contracts on Sourcify and on Hoodi's Blockscout (no
#                                  key) and adds the explorer links to deployments/<chainid>.json.
#
# The pins below are the settle's, which doesn't depend on the deployer's address; the deployments' gas moves by a
# few gas with the address bytes, so it is printed, not pinned. docs/EVM-GAS-MODEL.md §5 records them.
set -euo pipefail

cd "$(dirname "$0")/.."

rpc_url="${RPC_URL:-${HOODI_RPC_URL:-}}"
script=DeployTestnet.s.sol
hoodi=560048
osaka_fork_id=0x23aa1351 # Hoodi's eth_config forkId under Osaka (BPO2); anything else means a new plan
tx_gas_cap=16777216      # EIP-7825
run_id=0x058d7f0512ac5fa539c1c21941f2bde128e85cedb4106adcff30e601d41d6b28 # SPEC.md §1.4
zkvm_plonk=1                                                                # VerifierKind.ZKVM_PLONK
# The settle on Osaka through the canonical SP1VerifierPlonk: the receipt, execution (cast run's top frame) and
# the EIP-7623 floor, intrinsic included
settle_gas_used=525585
settle_execution_gas=411389
settle_floor_gas=253990
blockscout_api=https://eth-hoodi.blockscout.com/api/
# anvil's first dev account, the rehearsal's default deployer
anvil_account=0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
# What DeployTestnet.s.sol broadcasts, in order: transactionType, contractName and function
expected_txs='CREATE SashModelRegistry -
CALL SashModelRegistry enrollModel(bytes32,uint16,uint16,uint32,bytes32)
CREATE Sp1SashSegmentVerifier -
CREATE VerifiableInferenceSettlement -
CALL VerifiableInferenceSettlement verifyAndSettle(bytes32,(uint16,bytes32,bytes32,bytes32,bytes32,bytes32)[],uint32[],uint32[],uint32[],uint32[],bytes)'

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

for tool in anvil cast forge jq git; do
    command -v "$tool" >/dev/null 2>&1 || fail "'$tool' not found in PATH"
done

# The pinned verifier and its codehash, from the script that broadcasts against them
sp1_verifier=$(sed -n 's/^address constant SP1_VERIFIER_PLONK = \(0x[0-9a-fA-F]\{40\}\);$/\1/p' "script/$script")
sp1_codehash=$(sed -n 's/^bytes32 constant SP1_VERIFIER_PLONK_CODEHASH = \(0x[0-9a-f]\{64\}\);$/\1/p' "script/$script")
[[ -n "$sp1_verifier" && -n "$sp1_codehash" ]] || fail "cannot read the pinned SP1 verifier from script/$script"

lower() { tr '[:upper:]' '[:lower:]'; }

# cast with RPC URLs redacted from its error output: the URL can carry an API key.
redact_urls=()
c() {
    local err out rc=0 msg u
    err=$(mktemp)
    out=$(cast "$@" 2>"$err") || rc=$?
    if ((rc != 0)); then
        msg=$(cat "$err")
        for u in "${redact_urls[@]}"; do msg=${msg//"$u"/<rpc>}; done
        echo "$msg" >&2
    fi
    rm -f "$err"
    printf '%s\n' "$out"
    return "$rc"
}

need_rpc() {
    [[ -n "$rpc_url" ]] || fail "set RPC_URL or HOODI_RPC_URL (an archive endpoint, such as https://hoodi.drpc.org)"
    redact_urls+=("$rpc_url")
}

is_anvil() { cast rpc anvil_nodeInfo --rpc-url "$1" >/dev/null 2>&1; }

check_clean_tree() {
    [[ -z "$(git status --porcelain)" ]] || fail "the working tree is not clean (git status)"
}

foundry_version() {
    forge --version | awk '/^forge Version:/ {v = $3} /^Commit SHA:/ {s = $3} END {print v " (" s ")"}'
}

# check_eth_config JSON: Hoodi on Osaka, nothing scheduled
check_eth_config() {
    local fork_id next
    fork_id=$(jq -r '.current.forkId' <<<"$1")
    next=$(jq -c '.next' <<<"$1")
    [[ "$fork_id" == "$osaka_fork_id" ]] \
        || fail "eth_config forkId is $fork_id, not $osaka_fork_id (Osaka): a new fork means a new plan, not a record"
    [[ "$next" == null ]] || fail "eth_config schedules a next fork ($next): a new fork means a new plan, not a record"
}

# check_runtime_code NAME ADDRESS RPC: ADDRESS's runtime code is out/NAME.sol/NAME.json's deployedBytecode, the
# immutables' bytes (immutableReferences) masked on both sides; the metadata hash at its end binds the sources.
check_runtime_code() {
    local name=$1 address=$2 rpc=$3 artifact want got start len zeros
    artifact="out/$name.sol/$name.json"
    [[ -f "$artifact" ]] || fail "no $artifact"
    want=$(jq -r .deployedBytecode.object "$artifact" | lower)
    got=$(c code "$address" --rpc-url "$rpc" | lower)
    want=${want#0x}
    got=${got#0x}
    [[ ${#got} == "${#want}" ]] \
        || fail "$name at $address is $((${#got} / 2)) bytes, not the $((${#want} / 2)) of HEAD's build: not HEAD's code"
    while read -r start len; do
        zeros=$(printf "%0$((2 * len))d" 0)
        want=${want:0:2*start}$zeros${want:2*(start+len)}
        got=${got:0:2*start}$zeros${got:2*(start+len)}
    done < <(jq -r '.deployedBytecode.immutableReferences // {} | .[][] | "\(.start) \(.length)"' "$artifact")
    [[ "$got" == "$want" ]] || fail "$name at $address is not HEAD's build of src/sash/$name.sol (immutables aside)"
}

# calldata_counts HEX: prints the byte count, the zero bytes and the non-zero bytes
calldata_counts() {
    local input=$1 bytes zero
    bytes=$(((${#input} - 2) / 2))
    zero=$(fold -w2 <<<"${input:2}" | grep -c '^00$' || true)
    echo "$bytes $zero $((bytes - zero))"
}

# record RPC MODE OUT [PREFLIGHT]: MODE is live or fork; PREFLIGHT, when given, is folded into the record
record() {
    local rpc=$1 mode=$2 out=$3 preflight=${4:-}
    local settle_tx=${SETTLE_TX:-}

    local chain broadcast
    chain=$(c chain-id --rpc-url "$rpc")
    ((chain == hoodi)) || fail "RPC_URL serves chain $chain; only Hoodi ($hoodi) is pinned"
    broadcast="broadcast/$script/$chain/run-latest.json"
    [[ -f "$broadcast" ]] || fail "no $broadcast"
    [[ -z "$preflight" || -f "$preflight" ]] || fail "no $preflight: the rehearsal (make testnet-rehearsal) writes it before the broadcast"
    [[ "$(jq -r .chain "$broadcast")" == "$chain" ]] || fail "$broadcast is for chain $(jq -r .chain "$broadcast"), not $chain"

    # Every hash the broadcast sent exists on RPC_URL: a rehearsal's never does on the live chain.
    local n hashes h
    n=$(jq '.transactions | length' "$broadcast")
    hashes=$(jq -r '.transactions[].hash // empty' "$broadcast")
    for h in $hashes; do
        [[ "$(c rpc eth_getTransactionByHash "$h" --rpc-url "$rpc")" != null ]] \
            || fail "transaction $h of $broadcast does not exist on RPC_URL (a rehearsal's broadcast, or another chain's)"
    done

    # The commit the broadcast was built from is HEAD, clean, and on a live chain reachable from origin/main; the
    # deployed code is checked against HEAD's build below, since the tree is only clean now, not at the broadcast
    local commit head
    commit=$(jq -r .commit "$broadcast")
    head=$(git rev-parse HEAD)
    [[ "$commit" =~ ^[0-9a-f]{7,40}$ && "$head" == "$commit"* ]] \
        || fail "$broadcast was built from commit $commit, not HEAD $head"
    check_clean_tree
    if [[ "$mode" == live ]]; then
        git merge-base --is-ancestor HEAD origin/main \
            || fail "HEAD $head is not on origin/main (git fetch origin main, then check out the deployed commit)"
    fi
    forge build --locked >/dev/null || fail "forge build --locked failed"

    # The five transactions, in order, and nothing else (no DevTranscriptSegmentVerifier)
    local got_txs
    got_txs=$(jq -r '.transactions[] | "\(.transactionType) \(.contractName) \(.function // "-")"' "$broadcast")
    [[ "$(jq '[.transactions[] | select(.contractName == "DevTranscriptSegmentVerifier")] | length' "$broadcast")" == 0 ]] \
        || fail "$broadcast deploys a DevTranscriptSegmentVerifier"
    if [[ -n "$settle_tx" ]]; then
        # the broadcast's own settle may be missing: forge never sent it, or never wrote it
        [[ "$got_txs" == "$expected_txs" || "$got_txs" == "$(head -n 4 <<<"$expected_txs")" ]] \
            || fail "$broadcast does not hold the four deployment transactions and at most the settle; it holds:"$'\n'"$got_txs"
    else
        [[ "$got_txs" == "$expected_txs" ]] \
            || fail "$broadcast does not hold exactly the five expected transactions; it holds:"$'\n'"$got_txs"
    fi
    local deployer registry adapter settlement
    deployer=$(jq -r '.transactions[0].transaction.from' "$broadcast" | lower)
    [[ "$(jq -r '[.transactions[].transaction.from | ascii_downcase] | unique | length' "$broadcast")" == 1 ]] \
        || fail "$broadcast has more than one sender"
    registry=$(jq -r '.transactions[0].contractAddress' "$broadcast" | lower)
    adapter=$(jq -r '.transactions[2].contractAddress' "$broadcast" | lower)
    settlement=$(jq -r '.transactions[3].contractAddress' "$broadcast" | lower)
    [[ "$(jq -r '.transactions[1].transaction.to' "$broadcast" | lower)" == "$registry" ]] \
        || fail "enrollModel is not sent to the registry"
    if ((n == 5)); then
        [[ "$(jq -r '.transactions[4].transaction.to' "$broadcast" | lower)" == "$settlement" ]] \
            || fail "verifyAndSettle is not sent to the settlement"
    fi

    # The deployments, the broadcast's settle unless SETTLE_TX replaces it, and SETTLE_TX
    local own_settle settle
    own_settle=$(jq -r '.transactions[4].hash // empty' "$broadcast")
    if [[ -n "$settle_tx" ]]; then
        [[ "$settle_tx" =~ ^0x[0-9a-fA-F]{64}$ ]] || fail "SETTLE_TX is not a transaction hash"
        settle=$(lower <<<"$settle_tx")
        [[ "$settle" != "$own_settle" ]] || fail "SETTLE_TX is the broadcast's own settle; record it without SETTLE_TX"
        [[ "$(c rpc eth_getTransactionByHash "$settle" --rpc-url "$rpc")" != null ]] \
            || fail "SETTLE_TX $settle does not exist on RPC_URL"
    else
        settle=$own_settle
    fi
    local mined=() i
    for i in 0 1 2 3; do
        h=$(jq -r ".transactions[$i].hash // empty" "$broadcast")
        [[ -n "$h" ]] || fail "$broadcast is incomplete: finish the broadcast with forge script ... --resume"
        mined+=("$h")
    done
    if [[ -n "$own_settle" ]]; then mined+=("$own_settle"); fi
    if [[ -n "$settle_tx" ]]; then mined+=("$settle"); fi

    # Live: wait until every block involved is finalized, then read the canonical receipts, each checked below to
    # be in a finalized block of the canonical chain (a reorg during the wait can move a transaction).
    local receipt last_block=0 b finalized=0
    for h in "${mined[@]}"; do
        while [[ "$(c rpc eth_getTransactionReceipt "$h" --rpc-url "$rpc")" == null ]]; do
            [[ "$mode" == live ]] || fail "transaction $h is not mined"
            echo "waiting for transaction $h to be mined"
            sleep 12
        done
        b=$(cast to-dec "$(c rpc eth_getTransactionReceipt "$h" --rpc-url "$rpc" | jq -r .blockNumber)")
        if ((b > last_block)); then last_block=$b; fi
    done
    if [[ "$mode" == live ]]; then
        while :; do
            finalized=$(c block finalized -f number --rpc-url "$rpc")
            ((finalized >= last_block)) && break
            echo "waiting for finality: finalized block $finalized, the last transaction is in block $last_block"
            sleep 60
        done
        echo "finalized block $finalized covers block $last_block"
    fi

    # The fork rules: RPC_URL's eth_config, or on an anvil node the forked chain's (anvil reports its own,
    # forkId 0x00000000), read from the URL anvil forks, which is never printed
    local eth_config
    if [[ "$mode" == fork ]]; then
        local node fork_url
        node=$(c rpc anvil_nodeInfo --rpc-url "$rpc")
        [[ "$(jq -r .hardFork <<<"$node")" == Osaka ]] || fail "the anvil node runs $(jq -r .hardFork <<<"$node"), not Osaka"
        fork_url=$(jq -r '.forkConfig.forkUrl // empty' <<<"$node")
        [[ -n "$fork_url" ]] || fail "the anvil node is not a fork"
        redact_urls+=("$fork_url")
        eth_config=$(c rpc eth_config --rpc-url "$fork_url")
    else
        eth_config=$(c rpc eth_config --rpc-url "$rpc")
    fi
    check_eth_config "$eth_config"

    echo
    echo "$script on chain $chain ($mode), commit $commit, deployer $deployer"
    printf '%-52s %10s %10s %14s %8s %10s\n' "Transaction" "gasUsed" "gasLimit" "calldata (B)" "zero" "non-zero"
    local rows='{}' tx label gas_used gas_limit bytes zero nonzero block status from to
    local -a labels=("SashModelRegistry deployment" "SashModelRegistry.enrollModel" "Sp1SashSegmentVerifier deployment"
        "VerifiableInferenceSettlement deployment" "VerifiableInferenceSettlement.verifyAndSettle")
    local -a keys=(SashModelRegistry enrollment Sp1SashSegmentVerifier VerifiableInferenceSettlement)
    local -a addresses=("$registry" "" "$adapter" "$settlement")
    local settle_receipt="" settle_tx_json=""
    for i in "${!mined[@]}"; do
        h=${mined[$i]}
        tx=$(c rpc eth_getTransactionByHash "$h" --rpc-url "$rpc")
        receipt=$(c rpc eth_getTransactionReceipt "$h" --rpc-url "$rpc")
        status=$(jq -r .status <<<"$receipt")
        gas_used=$(cast to-dec "$(jq -r .gasUsed <<<"$receipt")")
        gas_limit=$(cast to-dec "$(jq -r .gas <<<"$tx")")
        block=$(cast to-dec "$(jq -r .blockNumber <<<"$receipt")")
        from=$(jq -r .from <<<"$tx" | lower)
        to=$(jq -r '.to // empty' <<<"$tx" | lower)
        read -r bytes zero nonzero <<<"$(calldata_counts "$(jq -r .input <<<"$tx")")"
        ((gas_limit <= tx_gas_cap)) || fail "transaction $h has gas limit $gas_limit > $tx_gas_cap"
        if [[ "$mode" == live ]]; then
            ((block <= finalized)) || fail "transaction $h is now in block $block, after the finalized block $finalized" \
                "(a reorg during the wait): run the record again"
            [[ "$(c block "$block" -f hash --rpc-url "$rpc")" == "$(jq -r .blockHash <<<"$receipt")" ]] \
                || fail "transaction $h's block $block is not the canonical one: run the record again"
        fi
        if ((i < 4)); then
            label=${labels[$i]}
            [[ "$from" == "$deployer" ]] || fail "transaction $h is not from the deployer $deployer"
            [[ "$status" == 0x1 ]] || fail "transaction $h ($label) reverted"
            if [[ -n "${addresses[$i]}" ]]; then
                [[ -z "$to" && "$(jq -r .contractAddress <<<"$receipt" | lower)" == "${addresses[$i]}" ]] \
                    || fail "transaction $h did not create ${addresses[$i]}"
                check_runtime_code "${keys[$i]}" "${addresses[$i]}" "$rpc"
                local code
                code=$(c code "${addresses[$i]}" --rpc-url "$rpc")
                rows=$(jq --arg k "${keys[$i]}" --arg a "$(cast to-check-sum-address "${addresses[$i]}")" --arg h "$h" \
                    --argjson b "$block" --argjson gu "$gas_used" --argjson gl "$gas_limit" \
                    --argjson size "$(((${#code} - 2) / 2))" --arg ch "$(c codehash "${addresses[$i]}" --rpc-url "$rpc")" \
                    '.[$k] = {address: $a, creationTx: $h, block: $b, gasUsed: $gu, gasLimit: $gl, runtimeSize: $size, codehash: $ch}' \
                    <<<"$rows")
            else
                [[ "$to" == "$registry" ]] || fail "transaction $h is not sent to the registry"
                rows=$(jq --arg h "$h" --argjson b "$block" --argjson gu "$gas_used" --argjson gl "$gas_limit" \
                    '.enrollment = {tx: $h, block: $b, gasUsed: $gu, gasLimit: $gl}' <<<"$rows")
            fi
        elif [[ "$h" == "$own_settle" && -n "$settle_tx" ]]; then
            # SETTLE_TX: the broadcast's own settle must have lost to the copy
            label="verifyAndSettle, the broadcast's (front-run)"
            [[ "$status" == 0x0 ]] || fail "the broadcast's own settle $h succeeded; record it without SETTLE_TX"
            local own_run sel
            own_run=$(c run "$h" --rpc-url "$rpc" 2>&1 || true)
            sel=$(cast sig "RunAlreadySettled(bytes32)")
            grep -q -e "RunAlreadySettled($run_id)" -e "${sel}${run_id#0x}" <<<"$own_run" \
                || fail "the broadcast's own settle $h did not revert with RunAlreadySettled($run_id)"
        else
            label=${labels[4]}
            if [[ -n "$settle_tx" ]]; then label="verifyAndSettle, SETTLE_TX"; fi
            [[ "$status" == 0x1 ]] || fail "the settle $h reverted"
            [[ "$to" == "$settlement" ]] || fail "the settle $h is sent to $to, not the settlement $settlement"
            settle_receipt=$receipt
            settle_tx_json=$tx
        fi
        printf '%-52s %10s %10s %14s %8s %10s\n' "$label" "$gas_used" "$gas_limit" "$bytes" "$zero" "$nonzero"
    done
    [[ -n "$settle_receipt" ]] || fail "no settle transaction"

    # Exactly one InferenceVerified from the settlement, with the run's runId and ZKVM_PLONK
    local event_sig events
    event_sig=$(cast keccak "InferenceVerified(bytes32,bytes32,bytes32,bytes32,bytes32,uint8)")
    events=$(jq -c --arg a "$settlement" --arg s "$event_sig" \
        '[.logs[] | select((.address | ascii_downcase) == $a and .topics[0] == $s)]' <<<"$settle_receipt")
    [[ "$(jq length <<<"$events")" == 1 ]] || fail "the settle emits $(jq length <<<"$events") InferenceVerified from $settlement, not one"
    [[ "$(jq -r '.[0].topics[1]' <<<"$events")" == "$run_id" ]] || fail "InferenceVerified carries the wrong runId"
    # data = promptHash, answerHash, verifierKind
    [[ "$(cast to-dec "0x$(jq -r '.[0].data' <<<"$events" | tail -c 65)")" == "$zkvm_plonk" ]] \
        || fail "InferenceVerified verifierKind is not ZKVM_PLONK ($zkvm_plonk)"

    # The stack's immutables, read back from the chain
    local live_sp1 vkey fixture_vkey
    [[ "$(c call "$registry" "OWNER()(address)" --rpc-url "$rpc" | lower)" == "$deployer" ]] \
        || fail "the registry's OWNER is not the deployer"
    live_sp1=$(c call "$adapter" "SP1_VERIFIER()(address)" --rpc-url "$rpc")
    [[ "$(lower <<<"$live_sp1")" == "$(lower <<<"$sp1_verifier")" ]] || fail "the adapter's SP1_VERIFIER is $live_sp1"
    [[ "$(c codehash "$sp1_verifier" --rpc-url "$rpc")" == "$sp1_codehash" ]] || fail "the SP1 verifier's codehash is not $sp1_codehash"
    vkey=$(c call "$adapter" "SASH_AGGREGATOR_VKEY()(bytes32)" --rpc-url "$rpc")
    fixture_vkey=$(jq -r .programVKey fixtures/tinyllama.plonk.json)
    [[ "$vkey" == "$fixture_vkey" ]] || fail "the adapter's SASH_AGGREGATOR_VKEY $vkey is not the fixture's $fixture_vkey"
    [[ "$(c call "$adapter" "verifierKind()(uint8)" --rpc-url "$rpc")" == "$zkvm_plonk" ]] || fail "the adapter's verifierKind is not ZKVM_PLONK"
    [[ "$(c call "$settlement" "REGISTRY()(address)" --rpc-url "$rpc" | lower)" == "$registry" ]] || fail "the settlement's REGISTRY is not the registry"
    [[ "$(c call "$settlement" "SEGMENT_VERIFIER()(address)" --rpc-url "$rpc" | lower)" == "$adapter" ]] \
        || fail "the settlement's SEGMENT_VERIFIER is not the adapter"
    [[ "$(c call "$settlement" "ALLOW_DEV_VERIFIER()(bool)" --rpc-url "$rpc")" == false ]] || fail "ALLOW_DEV_VERIFIER is not false"
    [[ "$(c call "$settlement" "settledRuns(bytes32)(bool)" "$run_id" --rpc-url "$rpc")" == true ]] || fail "the run is not settled"
    local version verifier_hash vk_root
    version=$(c call "$sp1_verifier" "VERSION()(string)" --rpc-url "$rpc" | jq -r .)
    verifier_hash=$(c call "$sp1_verifier" "VERIFIER_HASH()(bytes32)" --rpc-url "$rpc")
    vk_root=$(c call "$sp1_verifier" "VK_ROOT()(bytes32)" --rpc-url "$rpc")
    [[ "$verifier_hash" == "$(c call "$adapter" "SP1_PLONK_VERIFIER_HASH()(bytes32)" --rpc-url "$rpc")" ]] \
        || fail "the SP1 verifier's VERIFIER_HASH is not the adapter's SP1_PLONK_VERIFIER_HASH"
    [[ "$vk_root" == "$(c call "$adapter" "SP1_VK_ROOT()(bytes32)" --rpc-url "$rpc")" ]] \
        || fail "the SP1 verifier's VK_ROOT is not the adapter's SP1_VK_ROOT"

    # EIP-7623: gasUsed = max(21,000 + standard calldata + execution, 21,000 + floor). Execution is the top
    # frame of cast run's local replay, which replays the block's earlier transactions first (no --quick); its
    # "Gas used" line prints the receipt's total, so it is only a cross-check.
    local replay execution replay_total standard floor total trace_gas
    settle=$(jq -r .transactionHash <<<"$settle_receipt")
    replay=$(c run "$settle" --rpc-url "$rpc") || fail "cast run $settle failed"
    execution=$(awk '/^Traces:/ {getline; if (match($0, /\[[0-9]+\]/)) print substr($0, RSTART + 1, RLENGTH - 2); exit}' <<<"$replay")
    replay_total=$(sed -n 's/^Gas used: \([0-9]*\)$/\1/p' <<<"$replay")
    [[ -n "$execution" && -n "$replay_total" ]] || fail "cannot read the execution gas from cast run $settle"
    gas_used=$(cast to-dec "$(jq -r .gasUsed <<<"$settle_receipt")")
    ((replay_total == gas_used)) || fail "cast run's Gas used $replay_total != the receipt's gasUsed $gas_used"
    if trace_gas=$(cast rpc trace_transaction "$settle" --rpc-url "$rpc" 2>/dev/null | jq -r '.[0].result.gasUsed'); then
        ((execution == $(cast to-dec "$trace_gas"))) || fail "cast run's top frame $execution != trace_transaction's $(cast to-dec "$trace_gas")"
        trace_gas="; trace_transaction agrees"
    else
        trace_gas=""
    fi
    read -r bytes zero nonzero <<<"$(calldata_counts "$(jq -r .input <<<"$settle_tx_json")")"
    standard=$((4 * zero + 16 * nonzero))
    floor=$((21000 + 10 * zero + 40 * nonzero))
    total=$((21000 + standard + execution))
    if ((floor > total)); then total=$floor; fi
    echo "verifyAndSettle under EIP-7623: $zero zero, $nonzero non-zero calldata bytes; standard $standard," \
        "execution $execution (cast run's top frame$trace_gas), intrinsic + standard + execution" \
        "$((21000 + standard + execution)), floor (intrinsic included) $floor"
    ((gas_used == total)) || fail "verifyAndSettle gasUsed $gas_used != the EIP-7623 total $total"
    ((gas_used == settle_gas_used && execution == settle_execution_gas && floor == settle_floor_gas)) \
        || fail "the settle's gasUsed $gas_used, execution $execution and floor $floor are not the pinned" \
            "$settle_gas_used, $settle_execution_gas and $settle_floor_gas: stop and explain before recording"

    # The preflight file from the rehearsal: the same commit and deployer, read before the first deployment, and
    # predicting the deployed addresses
    local preflight_json=null
    if [[ -n "$preflight" ]]; then
        preflight_json=$(jq -c . "$preflight")
        local first_block
        first_block=$(jq -r '.SashModelRegistry.block' <<<"$rows")
        jq -e --argjson chain "$chain" --arg commit "$head" --arg d "$deployer" --argjson fb "$first_block" \
            --arg r "$registry" --arg a "$adapter" --arg s "$settlement" '
            .chainId == $chain and .commit == $commit and (.deployer | ascii_downcase) == $d
            and .ethConfig.block < $fb and .forkBlock < $fb
            and (.predicted.SashModelRegistry | ascii_downcase) == $r
            and (.predicted.Sp1SashSegmentVerifier | ascii_downcase) == $a
            and (.predicted.VerifiableInferenceSettlement | ascii_downcase) == $s' <<<"$preflight_json" >/dev/null \
            || fail "$preflight does not match the broadcast: chain, commit $head, deployer $deployer, blocks below" \
                "the first deployment block $first_block, and the predicted addresses must all match"
    fi

    local block_hash settle_from settle_block
    block_hash=$(jq -r .blockHash <<<"$settle_receipt")
    settle_block=$(cast to-dec "$(jq -r .blockNumber <<<"$settle_receipt")")
    settle_from=$(cast to-check-sum-address "$(jq -r .from <<<"$settle_tx_json")")
    jq -n -S \
        --argjson chain "$chain" --argjson config "$eth_config" --arg commit "$head" --arg foundry "$(foundry_version)" \
        --arg solc "$(forge config --json | jq -r .solc)" --arg deployer "$(cast to-check-sum-address "$deployer")" \
        --argjson rows "$rows" --arg sp1 "$(cast to-check-sum-address "$sp1_verifier")" --arg sp1_codehash "$sp1_codehash" \
        --arg version "$version" --arg verifier_hash "$verifier_hash" --arg vk_root "$vk_root" --arg vkey "$vkey" \
        --arg settle "$settle" --arg from "$settle_from" --argjson settle_block "$settle_block" --arg block_hash "$block_hash" \
        --argjson gas_limit "$(cast to-dec "$(jq -r .gas <<<"$settle_tx_json")")" --argjson gas_used "$gas_used" \
        --argjson zero "$zero" --argjson nonzero "$nonzero" --argjson standard "$standard" --argjson floor "$floor" \
        --argjson execution "$execution" --arg run_id "$run_id" --argjson kind "$zkvm_plonk" \
        --argjson preflight "$preflight_json" '
        {
            chainId: $chain,
            forkId: $config.current.forkId,
            forkActivationTime: $config.current.activationTime,
            commit: $commit,
            foundry: $foundry,
            solc: $solc,
            deployer: $deployer,
            contracts: ($rows | {SashModelRegistry, Sp1SashSegmentVerifier, VerifiableInferenceSettlement}),
            enrollment: $rows.enrollment,
            sp1Verifier: {address: $sp1, codehash: $sp1_codehash, version: $version, verifierHash: $verifier_hash, vkRoot: $vk_root},
            programVKey: $vkey,
            settle: {
                tx: $settle, from: $from, blockNumber: $settle_block, blockHash: $block_hash,
                gasLimit: $gas_limit, gasUsed: $gas_used, calldataZeroBytes: $zero, calldataNonZeroBytes: $nonzero,
                standardCalldataGas: $standard, floorGas: $floor, executionGas: $execution,
                runId: $run_id, verifierKind: $kind
            }
        } + (if $preflight == null then {} else {preflight: $preflight} end)' >"$out"
    jq empty "$out"
    if [[ -n "$preflight" && "$mode" == live ]]; then rm -f "$preflight"; fi
    echo "OK: ${#mined[@]} transactions checked; InferenceVerified(runId $run_id, ZKVM_PLONK) from $settlement;" \
        "verifyAndSettle gasUsed $gas_used (pinned), execution $execution (pinned), forkId $osaka_fork_id; wrote $out"
}

record_cmd() {
    need_rpc
    local chain
    chain=$(c chain-id --rpc-url "$rpc_url")
    if is_anvil "$rpc_url"; then
        [[ -n "${OUT:-}" ]] || fail "RPC_URL is an anvil node (a rehearsal): set OUT to the file to write, outside deployments/"
        local out_dir deployments_dir
        out_dir=$(cd "$(dirname "$OUT")" 2>/dev/null && pwd -P) || fail "OUT's directory $(dirname "$OUT") does not exist"
        deployments_dir=$(mkdir -p deployments && cd deployments && pwd -P)
        [[ "$out_dir/" != "$deployments_dir/"* ]] || fail "a rehearsal is never recorded in or under deployments/"
        record "$rpc_url" fork "$OUT"
    else
        mkdir -p deployments
        record "$rpc_url" live "deployments/$chain.json" "deployments/$chain.preflight.json"
    fi
}

rehearse_cmd() {
    need_rpc
    local deployer=${DEPLOYER:-$anvil_account}
    deployer=$(cast to-check-sum-address "$deployer" 2>/dev/null) || fail "DEPLOYER is not an address"
    is_anvil "$rpc_url" && fail "RPC_URL is an anvil node; the rehearsal forks the live chain itself"
    local chain
    chain=$(c chain-id --rpc-url "$rpc_url")
    ((chain == hoodi)) || fail "RPC_URL serves chain $chain; only Hoodi ($hoodi) is pinned"
    local dir
    for dir in "broadcast/$script/$chain" "cache/$script/$chain"; do
        [[ ! -e "$dir" ]] || fail "contracts/$dir exists: record a live broadcast first; a dry run's files are" \
            "cleared with rm -rf contracts/$dir/dry-run && rmdir contracts/$dir (README §3, step 5)"
    done
    check_clean_tree

    local port=${ANVIL_PORT:-8578} fork_rpc
    fork_rpc="http://127.0.0.1:$port"
    if cast chain-id --rpc-url "$fork_rpc" >/dev/null 2>&1; then
        fail "port $port already serves an RPC node; set ANVIL_PORT to a free port"
    fi

    # The live chain's fork rules, read before the fork, and the block they were read at
    local config_block eth_config
    config_block=$(c block-number --rpc-url "$rpc_url")
    eth_config=$(c rpc eth_config --rpc-url "$rpc_url")
    check_eth_config "$eth_config"

    forge clean
    forge build --locked >/dev/null || fail "forge build --locked failed"

    local tmp
    tmp=${TMPDIR:-/tmp}
    tmp=$(mktemp -d "${tmp%/}/testnet-rehearsal.XXXXXX")
    anvil --fork-url "$rpc_url" --hardfork osaka --enable-tx-gas-limit --port "$port" --quiet >"$tmp/anvil.log" 2>&1 &
    anvil_pid=$!
    rehearsal_tmp=$tmp
    rehearsal_chain=$chain
    trap rehearse_cleanup EXIT
    for _ in $(seq 300); do
        cast chain-id --rpc-url "$fork_rpc" >/dev/null 2>&1 && break
        kill -0 "$anvil_pid" 2>/dev/null || fail "anvil exited; is port $port free and RPC_URL an archive endpoint?"
        sleep 0.1
    done
    cast chain-id --rpc-url "$fork_rpc" >/dev/null 2>&1 || fail "anvil did not answer on port $port"
    local fork_block nonce
    fork_block=$(c rpc anvil_nodeInfo --rpc-url "$fork_rpc" | jq -r .forkConfig.forkBlockNumber)
    c rpc anvil_impersonateAccount "$deployer" --rpc-url "$fork_rpc" >/dev/null
    c rpc anvil_setBalance "$deployer" 0x56bc75e2d63100000 --rpc-url "$fork_rpc" >/dev/null # 100 ETH
    nonce=$(c nonce "$deployer" --rpc-url "$fork_rpc")
    echo "Hoodi fork at block $fork_block on anvil $(c rpc anvil_nodeInfo --rpc-url "$fork_rpc" | jq -r .hardFork)," \
        "$(foundry_version | sed 's/^/Foundry /'); deployer $deployer at nonce $nonce"

    # The registry, enrollModel, the adapter and the settlement take nonces n to n + 3
    jq -n -S --argjson chain "$chain" --arg commit "$(git rev-parse HEAD)" --arg foundry "$(foundry_version)" \
        --argjson config_block "$config_block" --argjson config "$eth_config" --argjson fork_block "$fork_block" \
        --arg deployer "$deployer" --argjson nonce "$nonce" \
        --arg r "$(cast compute-address "$deployer" --nonce "$nonce" | awk '{print $NF}')" \
        --arg a "$(cast compute-address "$deployer" --nonce $((nonce + 2)) | awk '{print $NF}')" \
        --arg s "$(cast compute-address "$deployer" --nonce $((nonce + 3)) | awk '{print $NF}')" '
        {
            chainId: $chain, commit: $commit, foundry: $foundry,
            ethConfig: {block: $config_block, result: $config},
            forkBlock: $fork_block, deployer: $deployer, deployerNonce: $nonce,
            predicted: {SashModelRegistry: $r, Sp1SashSegmentVerifier: $a, VerifiableInferenceSettlement: $s}
        }' >"$tmp/preflight.json"

    forge script "script/$script:${script%%.*}" --rpc-url "$fork_rpc" --unlocked --sender "$deployer" \
        --broadcast --slow >"$tmp/forge-script.log" 2>&1 || {
        tail -n 30 "$tmp/forge-script.log" >&2
        fail "forge script $script failed on the fork"
    }
    record "$fork_rpc" fork "$tmp/$chain.json" "$tmp/preflight.json"

    mkdir -p deployments
    cp "$tmp/preflight.json" "deployments/$chain.preflight.json"
    echo "OK: rehearsal recorded in $tmp/$chain.json; preflight written to contracts/deployments/$chain.preflight.json"
}

# The rehearsal's broadcast and cache are moved into its temp dir, never left where the live broadcast, record and
# `forge script --resume` look.
rehearse_cleanup() {
    kill "$anvil_pid" 2>/dev/null || true
    wait "$anvil_pid" 2>/dev/null || true
    local dir
    for dir in broadcast cache; do
        if [[ -e "$dir/$script/$rehearsal_chain" ]]; then
            mkdir -p "$rehearsal_tmp/$dir/$script"
            mv "$dir/$script/$rehearsal_chain" "$rehearsal_tmp/$dir/$script/"
            rmdir "$dir/$script" "$dir" 2>/dev/null || true
        fi
    done
    echo "rehearsal files: $rehearsal_tmp (broadcast/, cache/, the record and the forge and anvil logs)"
}

verify_cmd() {
    need_rpc
    local chain record_file
    chain=$(c chain-id --rpc-url "$rpc_url")
    ((chain == hoodi)) || fail "RPC_URL serves chain $chain; only Hoodi ($hoodi) is pinned"
    record_file="deployments/$chain.json"
    [[ -f "$record_file" ]] || fail "no $record_file: run make testnet-record first"
    [[ "$(jq -r .commit "$record_file")" == "$(git rev-parse HEAD)" ]] \
        || fail "HEAD is not the recorded commit $(jq -r .commit "$record_file"); check it out to verify its sources"
    # the record itself may be the one uncommitted change
    [[ -z "$(git status --porcelain --untracked-files=all | grep -v " contracts/$record_file$" || true)" ]] \
        || fail "the working tree has changes besides $record_file"
    forge clean
    forge build --locked >/dev/null || fail "forge build --locked failed"

    local registry adapter settlement deployer sp1 vkey
    registry=$(jq -r .contracts.SashModelRegistry.address "$record_file")
    adapter=$(jq -r .contracts.Sp1SashSegmentVerifier.address "$record_file")
    settlement=$(jq -r .contracts.VerifiableInferenceSettlement.address "$record_file")
    deployer=$(jq -r .deployer "$record_file")
    sp1=$(jq -r .sp1Verifier.address "$record_file")
    vkey=$(jq -r .programVKey "$record_file")
    local -a names=(SashModelRegistry Sp1SashSegmentVerifier VerifiableInferenceSettlement)
    local -a addrs=("$registry" "$adapter" "$settlement")
    local -a args=("$(cast abi-encode "constructor(address)" "$deployer")"
        "$(cast abi-encode "constructor(address,bytes32)" "$sp1" "$vkey")"
        "$(cast abi-encode "constructor(address,address,bool)" "$registry" "$adapter" false)")
    local explorers='{}' i
    for i in 0 1 2; do
        echo "Sourcify: ${names[$i]} at ${addrs[$i]}"
        forge verify-contract "${addrs[$i]}" "src/sash/${names[$i]}.sol:${names[$i]}" --chain "$chain" \
            --constructor-args "${args[$i]}" --verifier sourcify --watch
        echo "Blockscout: ${names[$i]} at ${addrs[$i]}"
        forge verify-contract "${addrs[$i]}" "src/sash/${names[$i]}.sol:${names[$i]}" --chain "$chain" \
            --constructor-args "${args[$i]}" --verifier blockscout --verifier-url "$blockscout_api" --watch
        explorers=$(jq --arg k "${names[$i]}" --arg chain "$chain" --arg a "${addrs[$i]}" --arg bs "${blockscout_api%/api/}" \
            '.[$k] = {sourcify: "https://repo.sourcify.dev/\($chain)/\($a)", blockscout: "\($bs)/address/\($a)?tab=contract"}' \
            <<<"$explorers")
    done
    local tmp
    tmp=$(mktemp)
    jq -S --argjson e "$explorers" '.explorers = $e' "$record_file" >"$tmp"
    mv "$tmp" "$record_file"
    echo "OK: the three contracts verified on Sourcify and Blockscout; links added to contracts/$record_file"
}

case "${1:-}" in
rehearse) rehearse_cmd ;;
record) record_cmd ;;
verify) verify_cmd ;;
*) fail "usage: $0 rehearse|record|verify (see the header)" ;;
esac
