#!/bin/bash
set -eu
set -o pipefail

# checks and commit changes in rust binding
if [[ `git status ./binding --porcelain` ]]; then
    echo "********** NOT ALL RUST BINDINGS COMMITTED, AUTO PUSH **********\n";
    git add ./binding
    git commit -m "GEN: commit rust binding"
    git push
fi;
