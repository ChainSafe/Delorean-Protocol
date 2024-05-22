#!/bin/bash

set -e

cat << EOF > $MANIFEST_FILE
accounts:
  alice: {}

rootnet:
  type: New
  # Balances and collateral are in atto
  validators:
    alice: '100'
  balances:
    # 100FIL is 100_000_000_000_000_000_000
    alice: '100000000000000000000'
  env:
    CMT_CONSENSUS_TIMEOUT_COMMIT: 1s
    FM_NETWORK: $FM_NETWORK
    FM_DB__STATE_HIST_SIZE: "$STATE_HIST_SIZE"
    FM_DB__COMPACTION_STYLE: "$COMPACTION_STYLE"
    FM_TESTING__PUSH_CHAIN_META: "$PUSH_CHAIN_META"

  nodes:
    bench:
      mode:
        type: Validator
        validator: alice
      seed_nodes: []
      ethapi: false
EOF
