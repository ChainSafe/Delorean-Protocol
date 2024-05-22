#!/bin/bash

# This script should be used as ENTRYPOINT for fendermint docker image.

CMD=$1

if [[ $CMD == 'ipc-cli' ]]; then
  ipc-cli "${@:2}"
else
  if (( $# == 0)); then
    exec fendermint
  else
    exec fendermint "$@"
  fi
fi
