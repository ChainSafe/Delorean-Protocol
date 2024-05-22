#!/bin/bash

set -e

PORT_RANGES_FROM=$(cat $MATERIALIZER_DIR/materializer-state.json | jq ".port_ranges.\"testnets/$TESTNET_ID/root/nodes/$NODE_ID\".from")
COMETBFT_RPC_PORT=$(($PORT_RANGES_FROM + 57))

latest_block_height() {
  curl -s http://localhost:30657/status \
    | jq -r ".result.sync_info.latest_block_height"
}

current_db_size() {
  du $ROCKSDB_DIR | awk '{print $1;}'
}

rm -rf $MEASUREMENTS_FILE

echo "appending measurements every ${MEASUREMENTS_PERIOD_SECS}s to ${MEASUREMENTS_FILE}; Ctrl+C to exit..."
START_TIME=$(date +%s)

while true
do
  TS=$(date +%s);
  LBH=$(latest_block_height);
  DBS=$(current_db_size);
  echo "{\"timestamp\": $TS, \"block_height\": $LBH, \"db_size_kb\": $DBS}" >> $MEASUREMENTS_FILE;
  sleep $MEASUREMENTS_PERIOD_SECS;
done
