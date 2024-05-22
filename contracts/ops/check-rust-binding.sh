#!/bin/bash
set -eu
set -o pipefail

# checks if there are changes in rust binding
if [[ `git status ./binding --porcelain` ]]; then 
    echo "!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!"
    echo "********** NOT ALL RUST BINDINGS COMMITTED, COMMIT THEM **********\n";
    git status ./binding --porcelain
    echo "!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!\n"
    exit 1;
fi;