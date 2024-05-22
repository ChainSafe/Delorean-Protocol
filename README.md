# InterPlanetary Consensus (IPC)

**‼️ All the modules in the IPC stack (including the contracts) haven't been audited, tested in depth, or otherwise verified. Moreover, the system is missing critical recovery functionality in case of crashes. There are multiple ways in which you may lose funds moved into an IPC subnet, and we strongly advise against deploying IPC on mainnet and/or using it with tokens with real value.**

IPC is a framework that enables on-demand horizontal scalability of networks, by deploying "subnets" running different consensus algorithms depending on the application's requirements. With IPC, dApps can reach planetary scale through recursively scalable subnets, sub-second transactions, robust compute workloads, and highly adaptable WebAssembly runtimes tailored to developer requirements.

Visit the [IPC project page](https://www.ipc.space/) for news and guides.

## Prerequisites

On Linux (links and instructions for Ubuntu):

- Install system packages: `sudo apt install build-essential clang cmake pkg-config libssl-dev protobuf-compiler git curl`.
- Install Rust. See [instructions](https://www.rust-lang.org/tools/install).
- Install cargo-make: `cargo install --force cargo-make`.
- Install Docker. See [instructions](https://docs.docker.com/engine/install/ubuntu/).
- Install Foundry. See [instructions](https://book.getfoundry.sh/getting-started/installation).

On MacOS:

- Install Xcode from App Store or terminal: xcode-select --install
- Install Homebrew: https://brew.sh/
- Install dependencies: brew install jq
- Install Rust: https://www.rust-lang.org/tools/install (if you have homebrew installed rust, you may need to uninstall that if you get errors in the build)
- Install Cargo make: cargo install --force cargo-make
- Install docker: https://docs.docker.com/desktop/install/mac-install/
- Install foundry: https://book.getfoundry.sh/getting-started/installation

## Building

```
# make sure that rust has the wasm32 target
rustup target add wasm32-unknown-unknown

# add your user to the docker group
sudo usermod -aG docker $USER && newgrp docker

# clone this repo and build
git clone https://github.com/consensus-shipyard/ipc.git
cd ipc
make

# building will generate the following binaries
./target/release/ipc-cli --version
./target/release/fendermint --version
```

## Run tests

```
make test
```

## Code organization

- `ipc/cli`: A Rust binary crate for our client `ipc-cli` application that provides a simple and easy-to-use interface to interact with IPC as a user and run all the processes required for the operation of a subnet.
- `ipc/provider` A Rust crate that implements the `IpcProvider` library. This provider can be used to interact with IPC from Rust applications (and is what the `ipc-cli` uses under the hood).
- `ipc/api`: IPC common types and utils.
- `ipc/wallet`: IPC key management and identity.
- `fendermint`: Peer implementation to run subnets based on Tendermint Core.
- `contracts`: A reference implementation of all the actors (i.e. smart contracts) responsible for the operation of the IPC (Inter-Planetary Consensus) protocol.
- `ipld`: IPLD specific types and libraries

## Documentation and Guides

**We've prepared a [quick start guide](https://docs.ipc.space/quickstarts/deploy-a-subnet) that will have you running and validating on your own subnet quickly, at the cost of detailed explanations.**

For further documentation, see:

- [docs/contracts.md](./docs/ipc/contracts.md) for instructions on how to deploy FEVM actors on subnets.
- [docs/usage.md](./docs/ipc/usage.md) for instructions on how to use the `ipc-cli` to interact with subnets (from managing your identities, to sending funds to a subnet).
- [docs/deploying-hierarchy.md](./docs/ipc/deploying-hierarchy.md) for instructions on how to deploy your own instance of IPC on a network.

If you are a developer, see:

- [docs/developers.md](./docs/ipc/developers.md) for useful tips and guides targeted for IPC developers.

## Connecting to a rootnet

You can deploy an IPC hierarchy from any compatible rootnet. The recommended option is to use Filecoin Calibration, but you can also deploy your own.

### Running a subnet in Calibration

Calibration is the primary testnet for Filecoin. It already hosts the IPC actors and can be used as a rootnet on which to deploy new subnets.

In order to use the `ipc-cli` with Calibration we need to have access to a full node syncing with the network. The easiest way to achieve this is to use a [public RPC](https://docs.filecoin.io/networks/calibration/rpcs/). You also need the addresses of the deployed contracts.

If it is the first time that you use your `ipc-cli`, to initialize cli configuration you can run `ipc-cli config init`. This will populate a new default config file in `~/.ipc/config.toml`.

The suggested configuration for the `ipc-cli` is:

```
keystore_path = "~/.ipc"

# Filecoin Calibration
[[subnets]]
id = "/r314159"

[subnets.config]
network_type = "fevm"
provider_http = "https://api.calibration.node.glif.io/rpc/v1"
gateway_addr = "0x1AEe8A878a22280fc2753b3C63571C8F895D2FE3"
registry_addr = "0x0b4e239FF21b40120cDa817fba77bD1B366c1bcD"
```

To be able to interact with Calibration and run new subnets, some FIL should be provided to, at least, the wallet that will be used by the `ipc-cli` to interact with IPC. You can request some tFIL for your address through the [Calibration Faucet](https://faucet.calibration.fildev.network/funds.html).

## Help

If you meet any obstacles join us in **#ipc-help** in the [Filecoin Slack workspace](https://filecoin.io/slack).
