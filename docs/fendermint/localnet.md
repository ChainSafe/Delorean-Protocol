# Local Testnets

Setting up a Fendermint local testnet is a way to get started quickly with IPC.

This guide offers two flavours:

- A single node deployment: useful for developing smart contracts and testing the APIs.
- A 4 node testnet: useful for testing consensus, checkpointing, and more.

## Prerequisites

On Linux (links and instructions for Ubuntu):

- Install Docker. See [instructions](https://docs.docker.com/engine/install/ubuntu/).
- Install Rust. See [instructions](https://www.rust-lang.org/tools/install).
- Install cargo-make: `cargo install --force cargo-make`.

## Docker images

These commands will pull various Docker images from remote repositories, including `fendermint:latest`, by default.

- To override which Fendermint Docker image to pull, set the `FM_DOCKER_TAG` env variable to the desired tag.
- To use a local Fendermint image, set the `FM_PULL_SKIP` env variable to some value, e.g. `FM_PULL_SKIP=true`.

## Single node deployment

To run IPC in the local rootnet just perform the following:

```bash
cargo make --makefile ./infra/fendermint/Makefile.toml testnode
```

It will create three docker containers (cometbft, fendermint, and eth-api).

To stop run the following:
```bash
cargo make --makefile ./infra/Makefile.toml testnode-down
```

## Local 4-nodes deployment

To run IPC in the local rootnet with 4 nodes perform the following command:

```bash
cargo make --makefile ./infra/Makefile.toml testnet
```

To stop the network:

```bash
cargo make --makefile ./infra/Makefile.toml testnet-down
```

The testnet contains four logical nodes. Each node consists of cometbft, fendermint, and ethapi containers.
The Docker internal network is `192.167.10.0/24`.

The Ethereum API is accessible on the following endpoints on the Docker internal network:

- `192.167.10.10:8545` or `ethapi-node0:8545`
- `192.167.10.11:8545` or `ethapi-node1:8545`
- `192.167.10.12:8545` or `ethapi-node2:8545`
- `192.167.10.13:8545` or `ethapi-node3:8545`

And on the following endpoints from the host machine:

- `127.0.0.1:8545`
- `127.0.0.1:8546`
- `127.0.0.1:8547`
- `127.0.0.1:8548`

## What's happening behind the scenes

> For a 4-node deployment.

The deployment process performs the following steps:

- Remove all Docker containers, files, networks, etc. from any previous deployments.
- Create all necessary directories.
- Initialize CometBFT testnet by creating `config` and `data` directories using `cometbft` tools.
- Read CometBFT nodes private keys, derive node IDs and store them in `config.toml` for each node.
- Create the `genesis` file for Fendermint.
- Share the genesis among all Fendermint nodes.
- Run Fendermint application in 4 containers.
- Run CometBFT in 4 containers.
- Run Eth API in 4 containers.
