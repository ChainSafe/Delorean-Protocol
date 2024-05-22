#!/bin/bash
set -eu
set -o pipefail

# Check if RPC_URL is set
if [[ -z "$RPC_URL" ]]; then
    echo "RPC_URL is not set. Sourcing .env file..."
    source .env
fi

# Make a JSON-RPC call to get the chain ID using curl
response=$(curl -s -X POST -H "Content-Type: application/json" --data '{
  "jsonrpc":"2.0",
  "method":"eth_chainId",
  "params":[],
  "id":1
}' $RPC_URL)

# Extract the chain ID from the response using jq (ensure jq is installed)
chain_id=$(echo $response | jq -r '.result')

# Double-check that this is a valid chain-id
if [[ "$chain_id" =~ ^0x[0-9a-fA-F]+$ ]]; then
   # Export the chain ID as an environmental variable
   export CHAIN_ID=$chain_id
   # Print the chain ID for verification (optional)
   echo "[*] Target network Chain ID: $CHAIN_ID"
else
  echo "[*] Error getting a valid hexadecimal chain ID"
  exit 1
fi
