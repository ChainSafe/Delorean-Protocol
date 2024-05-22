#!/bin/bash
set -eu
set -o pipefail

if [ $# -ne 1 ]
then
    echo "Expected a single argument with the output directory for the compiled contracts"
    exit 1
fi

OUTPUT=$1

# checks and commit changes in output artifacts
if [[ `git status $OUTPUT --porcelain` ]]; then
    echo "********** NOT ALL ABI ARTIFACTS ARE COMMITTED, AUTO PUSH **********\n";
    git add $OUTPUT
    git commit -m "GEN: commit ABI artifacts"
    git push
fi;
