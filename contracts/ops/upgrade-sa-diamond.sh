#!/bin/bash
# Upgrades IPC Subnet Actor Diamond Facets on an EVM-compatible subnet using hardhat
set -eu
set -o pipefail

if [ $# -ne 2 ]
then
    echo "Expected an argument with the name of the network to deploy (localnet, calibrationnet, mainnet) followed by an argument for the Subnet actor address to upgrade"
    exit 1
fi

NETWORK="$1"
SUBNET_ACTOR_ADDRESS="$2"

if [ "$NETWORK" = "auto" ]; then
  echo "[*] Automatically getting chainID for network"
  source ops/chain-id.sh
fi


npx hardhat upgrade-sa-diamond --network "${NETWORK}" --address "$SUBNET_ACTOR_ADDRESS"
