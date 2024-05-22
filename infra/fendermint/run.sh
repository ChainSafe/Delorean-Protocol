#!/usr/bin/env bash
set -eu
set -o pipefail

if [ $# -ne 1 ]; then
  echo "usage: $0 (start|stop)"
  exit 1
fi

if [ -z ${NETWORK_NAME} ]; then
  echo "NETWORK_NAME variable is not set";
  exit 1
fi

export NETWORK_NAME

PORT1=26656
PORT2=26657
PORT3=8545

ACTION=

case $1 in
  start) ACTION="up -d" ;;
  stop)  ACTION="down" ;;
  *)
    echo "usage: $0 (start|stop)"
    exit 1
    ;;
esac

if [ "$1" == "start" ]; then
  # we need to remove the network with the same name
  # because that network might be created without subnet with necessary IP address space
  docker network rm -f ${NETWORK_NAME}
  docker network create --subnet 192.167.0.0/16 ${NETWORK_NAME}
fi

for i in $(seq 0 3); do
	export NODE_ID=${i}
	export PORT1
	export PORT2
	export PORT3
	export CMT_NODE_ADDR=192.167.10.$((${i}+2))
	export FMT_NODE_ADDR=192.167.10.$((${i}+6))
	export ETHAPI_NODE_ADDR=192.167.10.$((${i}+10))
	docker compose -f ./docker-compose.yml -p testnet_node_${i} $ACTION &
	PORT1=$((PORT1+3))
	PORT2=$((PORT2+3))
	PORT3=$((PORT3+1))
done

wait $(jobs -p)

if [ "$1" == "stop" ]; then
  docker network rm -f ${NETWORK_NAME}
fi
