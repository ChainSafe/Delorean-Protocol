#!/usr/bin/env bash

# A docker entry point that allows us to source multiple env-var files and kick off a command.

# Example:
# echo 'a=1' > test1.env
# echo 'b=2' > test2.env
# echo 'echo a=$a b=$b c=$1' > test.sh
# chmod +x test.sh
# fendermint/testing/materializer/scripts/docker-entry.sh "./test.sh 3"  test1.env test2.env

set -e

COMMAND=$1
shift

# Export all variables from all environment file args.
while [ ! -z $1 ]; do
  set -a
  source $1
  set +a
  shift
done

# Execute the real command, transfering the PID so it receives signals.
exec $COMMAND
