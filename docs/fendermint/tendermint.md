# Tendermint

To implement the [architecture](./architecture.md) we intend to make use of the following open source components to integrate with Tendermint:

* [CometBFT](https://github.com/cometbft/cometbft): One of the successors of the generic blockchain SMR system [Tendermint Core](https://github.com/tendermint/tendermint) (others might be listed [here](https://github.com/tendermint/ecosystem)). Originally we used the upcoming [v0.37](https://github.com/tendermint/tendermint/tree/v0.37.0-rc2), but it was eventually released as part of CometBFT. This version has the required [extensions](./architecture.md#abci) to [ABCI++](https://github.com/cometbft/cometbft/tree/v0.37.1/spec/abci).
* [tendermint-rs](https://github.com/informalsystems/tendermint-rs/) is a Rust library that contains Tendermint [datatypes](https://github.com/informalsystems/tendermint-rs/tree/main/tendermint); the [proto](https://github.com/informalsystems/tendermint-rs/tree/main/proto) code [generated](https://github.com/informalsystems/tendermint-rs/tree/main/tools/proto-compiler) from the Tendermint protobuf definitions; a synchronous [ABCI server](https://github.com/informalsystems/tendermint-rs/tree/main/abci) with a trait the application can implement, with a [KV-store example](https://github.com/informalsystems/tendermint-rs/blob/main/abci/src/application/kvstore/main.rs) familiar from the tutorial; and various other goodies for building docker images, integration testing the application with Tendermint, and so on. Since [this PR](https://github.com/informalsystems/tendermint-rs/pull/1193) the library supports both `v0.34` (the last stable release of Tendermint Core) and the newer `v0.37` version (the two are not wire compatible).
* [tower-abci](https://github.com/penumbra-zone/tower-abci) from Penumbra adapts the ABCI interfaces from `tendermint-rs` to be used with [tower](https://crates.io/crates/tower) and has a [server](https://github.com/penumbra-zone/tower-abci/blob/v0.7.0/src/v037/server.rs) implementation that works with the the `v0.37` wire format and uses [tokio](https://crates.io/crates/tokio). So, unlike the ABCI server in `tendermint-rs`, this is asynchronous, which we can make use of if we want to inject our own networking.

That should be enough to get us started with Tendermint.


## Install CometBFT

We will need ~~Tendermint Core~~ CometBFT running and building the blockchain, and since we don't want to fork it, we can install the pre-packaged `cometbft` binary from the [releases](https://github.com/cometbft/cometbft/releases). At the time of this writing, our target is the [v0.37.1](https://github.com/cometbft/cometbft/releases/tag/v0.37.1) release, and things should work with the `v0.37.0-rc2` version of Tendermint Core as well.

Alternatively, we can [install](https://github.com/cometbft/cometbft/blob/main/docs/guides/install.md) the project from source. I expect to have to dig around in the source code to understand the finer nuances, so this is what I'll do. It needs `go` 1.18 or higher [installed](https://go.dev/doc/install) (check with `go version`).

The following code downloads the source, checks out the branch with the necessary ABCI++ features, and installs it.
```shell
git clone https://github.com/cometbft/cometbft.git
cd cometbft
git checkout v0.37.1
make install
```

Check that the installation worked:

```console
$ cometbft version
0.37.1+2af25aea6
```

After this we can follow the [quick start guide](https://github.com/cometbft/cometbft/blob/main/docs/guides/quick-start.md#initialization) to initialize a local node and try out the venerable `kvstore` application.

Create the genesis files under `$HOME/.cometbft`:

```shell
cometbft init
```

Start a node; we'll see blocks being created every second:

```shell
cometbft node --proxy_app=kvstore
```

Then, from another terminal, send a transaction:

```shell
curl -s 'localhost:26657/broadcast_tx_commit?tx="foo=bar"'
```

Finally, query the value of the key we just added:

```shell
curl -s 'localhost:26657/abci_query?data="foo"' | jq -r ".result.response.value | @base64d"
```

We should see `bar` printed on the console.

Nice! The status of the node can be checked like so:

```shell
curl -s localhost:26657/status
```

To start from a clean slate, we can just clear out the data directory and run `tendermint init` again:

```shell
rm -rf ~/.cometbft
```

Alternatively, once we have our own genesis set up, we can discard just the blockchain data with the following command:

```shell
cometbft unsafe-reset-all
```

## Sanity check the Rust libraries

This is an optional step to check that the branch that we'll need to be using from `tendermint-rs` works with our chosen version of `cometbft`. In practice we'll just add a library reference to the github project until it's released, we don't have to clone the project. But it's useful to do so, to get familiar with the code.

```shell
git clone git@github.com:informalsystems/tendermint-rs.git
cd tendermint-rs
git checkout v0.31.0
```

Then, go into the `abci` crate to try the [example](https://github.com/informalsystems/tendermint-rs/tree/main/abci#examples) with the `kvstore` that, unlike previously, will run external to `tendermint`:

```shell
cd abci
```

Build and run the store:

```shell
cargo run --bin kvstore-rs --features binary,kvstore-app
```

Go back to the terminal we used to run `cometbft` and do what they suggest.

First ensure we have the genesis files:

```shell
cometbft init
```

Then try to run Tendermint; it's supposed to connect to `127.0.0.1:26658` where the store is running, and bind itself to `127.0.0.1:26657`:

```shell
cometbft unsafe-reset-all && cometbft start
```

This seems to work; in the CometBFT logs we see the process producing blocks:

```console
❯ cometbft unsafe-reset-all && cometbft start
...
I[2023-05-19|09:22:50.094] ABCI Handshake App Info                      module=consensus height=0 hash=00000000000000000000000000000000 software-version=0.1.0 protocol-version=1
I[2023-05-19|09:22:50.094] ABCI Replay Blocks                           module=consensus appHeight=0 storeHeight=0 stateHeight=0
I[2023-05-19|09:22:50.097] Completed ABCI Handshake - CometBFT and App are synced module=consensus appHeight=0 appHash=00000000000000000000000000000000
...
I[2023-05-19|09:22:53.193] received complete proposal block             module=consensus height=3 hash=CBAEA6A06C09F6E5D5D4C09315EBFA770FE75E165CDABABD95F91DBFF6E6AFF2
I[2023-05-19|09:22:53.208] finalizing commit of block                   module=consensus height=3 hash=CBAEA6A06C09F6E5D5D4C09315EBFA770FE75E165CDABABD95F91DBFF6E6AFF2 root=00 num_txs=0
I[2023-05-19|09:22:53.215] executed block                               module=state height=3 num_valid_txs=0 num_invalid_txs=0
I[2023-05-19|09:22:53.222] committed state                              module=state height=3 num_txs=0 app_hash=00
I[2023-05-19|09:22:53.229] indexed block exents                         module=txindex height=3
...
```

We can see the opposite side of the error in the console of the store:

```console
❯ cargo run --bin kvstore-rs --features binary,kvstore-app
    ...
2023-05-19T08:22:14.247867Z  INFO tendermint_abci::server: ABCI server running at 127.0.0.1:26658
2023-05-19T08:22:50.079635Z  INFO tendermint_abci::server: Incoming connection from: 127.0.0.1:44564
2023-05-19T08:22:50.079749Z  INFO tendermint_abci::server: Incoming connection from: 127.0.0.1:44572
2023-05-19T08:22:50.079843Z  INFO tendermint_abci::server: Listening for incoming requests from 127.0.0.1:44564
2023-05-19T08:22:50.079879Z  INFO tendermint_abci::server: Incoming connection from: 127.0.0.1:44576
2023-05-19T08:22:50.079887Z  INFO tendermint_abci::server: Listening for incoming requests from 127.0.0.1:44572
2023-05-19T08:22:50.079996Z  INFO tendermint_abci::server: Incoming connection from: 127.0.0.1:44590
2023-05-19T08:22:50.080002Z  INFO tendermint_abci::server: Listening for incoming requests from 127.0.0.1:44576
2023-05-19T08:22:50.080110Z  INFO tendermint_abci::server: Listening for incoming requests from 127.0.0.1:44590
2023-05-19T08:22:51.154772Z  INFO tendermint_abci::application::kvstore: Committed height 1
2023-05-19T08:22:52.183919Z  INFO tendermint_abci::application::kvstore: Committed height 2
2023-05-19T08:22:53.222453Z  INFO tendermint_abci::application::kvstore: Committed height 3
...
```

Try to send a transaction:

```console
❯ curl 'http://127.0.0.1:26657/broadcast_tx_async?tx="somekey=somevalue"'
{"jsonrpc":"2.0","id":-1,"result":{"code":0,"data":"","log":"","codespace":"","hash":"17ED61261A5357FEE7ACDE4FAB154882A346E479AC236CFB2F22A2E8870A9C3D"}}
```

and a query:

```console
❯ curl 'http://127.0.0.1:26657/abci_query?data=0x736f6d656b6579'
{"jsonrpc":"2.0","id":-1,"result":{"response":{"code":0,"log":"exists","info":"","index":"0","key":"c29tZWtleQ==","value":"c29tZXZhbHVl","proofOps":null,"height":"300","codespace":""}}}
```

All balls!

Now just make sure we clean up the compilation artifacts:

```shell
cargo clean
```
