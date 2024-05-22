#!/bin/bash
# Compile contract and output core contracts ABI
set -eu
set -o pipefail

if [ $# -ne 1 ]
then
    echo "Expected a single argument with the output directory for the compiled contracts"
    exit 1
fi

OUTPUT=$1

echo -e "\033[0;36mRunning a recursive submodule update to ensure build reproducibility with CI. Local uncommitted submodule changes will be overridden.\033[0m"
git submodule update --init --recursive
echo "[*] Compiling contracts and output core contracts ABI in $OUTPUT" 
forge build -C ./src/ -R $(jq '.remappings | join(",")' remappings.json) --lib-paths lib/ --via-ir --sizes --skip test --out=$OUTPUT
