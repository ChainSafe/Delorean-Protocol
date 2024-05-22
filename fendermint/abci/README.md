# ABCI Adapter

This library borrows from `tendermint-rs/abci` to define an async `Application` trait, and adapts it to the interface `tower-abci` expects, so that we can use the `Server` in `tower-abci` to serve requests coming from Tendermint Core.

## Example

See the [kvstore](./examples/kvstore.rs) for using it. To try, you'll need [tendermint](../../docs/tendermint.md).

Start the `kvstore`:

```shell
cargo run --example kvstore
```

Start `tendermint`:

```shell
tendermint unsafe-reset-all && tendermint start
```

Send a transaction:

```shell
curl -s 'localhost:26657/broadcast_tx_commit?tx="foo=bar"'
```

Send a query:

```console
$ curl -s 'localhost:26657/abci_query?data="foo"' | jq -r ".result.response.value | @base64d"
bar
```

If all goes well, logs should look like this:

```console
❯ cargo run --example kvstore

    Finished dev [unoptimized + debuginfo] target(s) in 0.10s
     Running `target/debug/examples/kvstore`
2023-01-13T11:35:50.444279Z  INFO tower_abci::server: starting ABCI server addr="127.0.0.1:26658"
2023-01-13T11:35:50.444411Z  INFO tower_abci::server: bound tcp listener local_addr=127.0.0.1:26658
2023-01-13T11:35:54.099202Z  INFO tower_abci::server: listening for requests
2023-01-13T11:35:54.099353Z  INFO tower_abci::server: listening for requests
2023-01-13T11:35:54.099766Z  INFO tower_abci::server: listening for requests
2023-01-13T11:35:54.099836Z  INFO tower_abci::server: listening for requests
2023-01-13T11:35:55.200926Z  INFO kvstore: commit retain_height=block::Height(0)
2023-01-13T11:35:56.237514Z  INFO kvstore: commit retain_height=block::Height(1)
2023-01-13T11:35:57.323522Z  INFO kvstore: commit retain_height=block::Height(2)
2023-01-13T11:35:58.309203Z  INFO kvstore: update key="foo" value="bar"
2023-01-13T11:35:58.314724Z  INFO kvstore: commit retain_height=block::Height(3)
```
