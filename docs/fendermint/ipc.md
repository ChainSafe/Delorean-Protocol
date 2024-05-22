# IPC

This documentation will guide you through the different utils provided in Fendermint for the deployment of Fendermint-based IPC subnets. All node processes are run inside Docker containers in your local environment.

This docs are only focused on the infrastructure deployment, for an end-to-end walk through of spawning IPC subnets refer to the [IPC quickstart](https://github.com/consensus-shipyard/ipc/blob/main/docs/quickstart-calibration.md).

## Prerequisites

* Install the basic requirements for IPC (see [README](../../README.md#Prerequisites))

## Deploy subnet bootstrap
In order not to expose directly the network address information from validators, subnets leverage the use of bootstrap nodes (or `seeds` in CometBFT parlance), for new nodes to discover peers in the network and connect to the subnet's validators. To run a bootstrap node you can run the following command from the root of the repo:
```bash
cargo make --makefile infra/Makefile.toml \
    -e SUBNET_ID=<SUBNET_ID> \
    -e CMT_P2P_HOST_PORT=<COMETBFT_P2P_PORT> \
    -e CMT_RPC_HOST_PORT=<COMETBFT_RPC_PORT> \
    -e BOOTSTRAPS=<BOOTSTRAP_ENDPOINT>
    -e PARENT_REGISTRY=<PARENT_REGISTRY_CONTRACT_ADDR> \
    -e PARENT_GATEWAY=<GATEWAY_REGISTRY_CONTRACT_ADDR> \
    -e CMT_P2P_EXTERNAL_ADDR=<COMETBFT_EXTERNAL_ENDPOINT> \
    bootstrap
```
You'll see that by the end of the output, this command should output the network address of your bootstrap. You can use this endpoint to include this bootstrap node as a seed in the `seeds` configuration of CometBFT.
```console
[cargo-make] INFO - Running Task: cometbft-wait
[cargo-make] INFO - Running Task: cometbft-node-id
2b23b8298dff7711819172252f9df3c84531b1d9@193.29.200.123:26656
[cargo-make] INFO - Build Done in 13.38 seconds.
```

If at any time you need to query the endpoint of your bootstrap, you can run:
```bash
cargo make --makefile infra/Makefile.toml \
    bootstrap-id
```

`cargo-make bootstrap` supports the following environment variables to customize the deployment:
- `CMT_P2P_HOST_PORT` (optional): Specifies the listening port for the bootstraps P2p interface in the localhost for CometBFT. This is the address that needs to be shared with other peers if they want to use the bootstrap as a `seed` to discover connections.
- `CMT_RPC_HOST_PORT` (optional): Specifies the listening port in the localhost for CometBFT's RPC.
- `SUBNET_ID`: SubnetID the bootstrap is operating in.
- `NODE_NAME` (optional): Node name information to attach to the containers of the deployment. This will be needed to deploy more than one bootstrap in the same local environment.
- `BOOTSTRAPS`: Comma separated list of bootstraps (or seeds in CometBFT parlance) that we want this bootstrap to also be connected to.
- `CMT_P2P_EXTERNAL_ADDR`: Address to advertise to peers for them to dial. If empty, will use the same as the default listening address from CometBFT (generally `0.0.0.0:<P2P_RPC_PORT>`).
- `PARENT_ENDPOINT`: Public endpoint that the validator should use to connect to the parent.
- `PARENT_REGISTRY`: Ethereum address of the IPC registry contract in the parent
- `PARENT_GATEWAY`: Ethereum address of the IPC gateway contract in the parent.

Finally, to remove the bootstrap you can run:
```bash
cargo make --makefile infra/Makefile.toml bootstrap-down
```
And to restart it:
```
cargo make --makefile infra/Makefile.toml bootstrap-restart
```


## Deploy child subnet validator
Once a child subnet has been bootstrapped in its parent, its subnet actor has been deployed, and has fulfilled its minimum requirements in terms of validators and minimum collateral, validators in the subnet can deploy their infrastructure to spawn the child subnet.

In order to spawn a validator node in a child subnet, you need to run:
```bash
cargo make --makefile infra/Makefile.toml \
    -e PRIVATE_KEY_PATH=<VALIDATOR_PRIV_KEY> \
    -e SUBNET_ID=<SUBNET_ID> \
    -e CMT_P2P_HOST_PORT=<COMETBFT_P2P_PORT> \
    -e CMT_RPC_HOST_PORT=<COMETBFT_RPC_PORT> \
    -e ETHAPI_HOST_PORT=<ETH_RPC_PORT> \
    -e BOOTSTRAPS=<BOOTSTRAP_ENDPOINT>
    -e PARENT_REGISTRY=<PARENT_REGISTRY_CONTRACT_ADDR> \
    -e PARENT_GATEWAY=<GATEWAY_REGISTRY_CONTRACT_ADDR> \
    -e CMT_P2P_EXTERNAL_ADDR=<COMETBFT_EXTERNAL_ENDPOINT> \
    child-validator
```
This command will run the infrastructure for a Fendermint validator in the child subnet. It will generate the genesis of the subnet from the information in its parent, and will run the validator's infrastructure with the specific configuration passed in the command.

`cargo-make child-validator` supports the following environment variables to customize the deployment:
- `CMT_P2P_HOST_PORT` (optional): Specifies the listening port in the localhost for the P2P interface of the CometBFT node.
- `CMT_RPC_HOST_PORT` (optional): Specifies the listening port in the localhost for CometBFT's RPC.
- `ETHAPI_HOST_PORT` (optional): Specifies the listening port in the localhost for the ETH RPC of the node.
- `NODE_NAME` (optional): Name for the node deployment. Along with `CMT_P2P_HOST_PORT`, `CMT_RPC_HOST_PORT` and `ETHAPI_HOST_PORT`, these variables come really handy for the deployment of several validator nodes over the same system.
- `PRIVATE_KEY_PATH`: Path of the hex encoded private key for your validator (it should be the corresponding one used to join the subnet in the parent). This can be exported from the `ipc-cli` or any other wallet like Metamask.
- `SUBNET_ID`: SubnetID for the child subnet.
- `BOOTSTRAPS`: Comma separated list of bootstraps (or seeds in CometBFT parlance).
- `CMT_P2P_EXTERNAL_ADDR`: Address to advertise to peers for them to dial. If empty, will use the same as the default listening address from CometBFT (generally `0.0.0.0:<P2P_RPC_PORT>`).
- `PARENT_ENDPOINT`: Public endpoint that the validator should use to connect to the parent.
- `PARENT_REGISTRY`: Ethereum address of the IPC registry contract in the parent
- `PARENT_GATEWAY`: Ethereum address of the IPC gateway contract in the parent.

Finally, to remove the bootstrap you can run:
```
cargo make --makefile infra/Makefile.toml child-validator-down
```
And to restart it:
```
cargo make --makefile infra/Makefile.toml child-validator-restart
```

## Deploy subnet full-node
To deploy a full node (i.e. a node that validates and keeps all the state of a subnet but doesn't participate in the proposal of new blocks), the following command can be used:
```bash
cargo make --makefile infra/Makefile.toml \
    -e SUBNET_ID=<SUBNET_ID> \
    -e CMT_P2P_HOST_PORT=<COMETBFT_P2P_PORT> \
    -e CMT_RPC_HOST_PORT=<COMETBFT_RPC_PORT> \
    -e ETHAPI_HOST_PORT=<ETH_RPC_PORT> \
    -e BOOTSTRAPS=<BOOTSTRAP_ENDPOINT>
    -e PARENT_REGISTRY=<PARENT_REGISTRY_CONTRACT_ADDR> \
    -e PARENT_GATEWAY=<GATEWAY_REGISTRY_CONTRACT_ADDR> \
    -e CMT_P2P_EXTERNAL_ADDR=<COMETBFT_EXTERNAL_ENDPOINT> \
    child-fullnode
```
The full node also has its corresponding commands to kill and restart the node:
```
cargo make --makefile infra/Makefile.toml child-fullnode-down
cargo make --makefile infra/Makefile.toml child-fullnode-restart
```
