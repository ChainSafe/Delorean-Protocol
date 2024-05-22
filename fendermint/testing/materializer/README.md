# Tesnet Materializer

The `materializer` is a crate to help provision testnets based on a _manifest_, using a backend such as Docker.

See the following for more details:
* the [manifest](./src/manifest.rs) format and corresponding [golden files](./golden/manifest)
* the [testnet](./src/testnet.rs) that walks the manifests and instructs the materializer to provision resources
* the [materializer](./src/materializer.rs) which is the abstract interface that backends need to implement
* the [docker](./src/docker) implementation of the materializer
* the [test manifests](./tests/manifests) used in integration testing
* the [docker tests](./tests/docker_tests) which provision local testnets and run tests against their nodes
* the [CLI commands](../../app/options/src/materializer.rs) to use the materializer outside the tests

## Usage


### Validate

The following is an example of using the CLI to validate one of the test manifests:

```bash
cargo run -q -p fendermint_app -- \
  materializer \
    --data-dir $PWD/testing/materializer/tests/docker-materializer-data/ \
    validate \
      --manifest-file ./testing/materializer/tests/manifests/layer2.yaml
```

It runs the testnet with a materalizer implementations that does basic sanity checking, but doesn't actually provision resources.

### Setup

The next command starts the testnet in local docker containers:

```bash
cargo run -q -p fendermint_app -- \
  materializer \
    --data-dir $PWD/testing/materializer/tests/docker-materializer-data/ \
    setup \
      --manifest-file ./testing/materializer/tests/manifests/layer2.yaml
```

Once the containers are running, we can use the following command to list them:

```console
❯ docker ps -a --format "table {{.ID}}\t{{.Names}}\t{{.Status}}\t{{.Ports}}" --filter "label=testnet=testnets/layer2"
CONTAINER ID   NAMES                          STATUS         PORTS
90434a219f76   piccadilly-relayer-9b4a1a      Up 3 minutes   8445/tcp, 26658/tcp
ba65a3e7383b   euston-relayer-aa2681          Up 3 minutes   8445/tcp, 26658/tcp
2c89964eb76a   manchester-ethapi-a53465       Up 4 minutes   26658/tcp, 0.0.0.0:30445->8445/tcp, :::30445->8445/tcp
232cfc9e80cf   manchester-cometbft-a53465     Up 4 minutes   26660/tcp, 0.0.0.0:30456->26656/tcp, :::30456->26656/tcp, 0.0.0.0:30457->26657/tcp, :::30457->26657/tcp
77a9e209f3f6   manchester-fendermint-a53465   Up 4 minutes   8445/tcp, 26658/tcp
8edb7417cb53   london-ethapi-05d823           Up 4 minutes   26658/tcp, 0.0.0.0:30345->8445/tcp, :::30345->8445/tcp
39c48e3aa670   london-cometbft-05d823         Up 4 minutes   26660/tcp, 0.0.0.0:30356->26656/tcp, :::30356->26656/tcp, 0.0.0.0:30357->26657/tcp, :::30357->26657/tcp
8d379fd40625   london-fendermint-05d823       Up 4 minutes   8445/tcp, 26658/tcp
29b7643bc25d   brussels-ethapi-955632         Up 4 minutes   26658/tcp, 0.0.0.0:30245->8445/tcp, :::30245->8445/tcp
9c3bd0e91f2b   brussels-cometbft-955632       Up 5 minutes   26660/tcp, 0.0.0.0:30256->26656/tcp, :::30256->26656/tcp, 0.0.0.0:30257->26657/tcp, :::30257->26657/tcp
355f567faba6   brussels-fendermint-955632     Up 5 minutes   8445/tcp, 26658/tcp
```

The names contain some hashed part that makes them unique, but with their prefix they will be recognisable in the manifest.

#### Artifacts

As the command indicates we can find the artifacts created by the docker materializer in `./tests/docker-materializer-data`, for example:
* the `materializer-state.json` file contains the mappings from node names to port ranges on the host
* the `testnets` directory contains all the testnets, inheriting the names of the manifest they come from
* the `testnets/<name>/ipc` directory contains the configuration for the `ipc-cli`, namely its `config.toml` and the `evm_keystore.json`
* the `testnets/<name>/root` directory is the rootnet of the testnet
* the `testnets/<name>/accounts/<account-name>` directory contains the public and private keys of an account
* the `testnets/<name>/root/subnets/<subnet-name>/subnet-id` file contains the `SubnetID` allocated to the subnet
* `testnets/<name>/root/subnets/<subnet-name>/genesis.json` is the Genesis constructed from the parent
* `testnets/<name>/root/subnets/<subnet-name>/nodes/<node-name>/static.env` contains environment variables for the node


#### Use `curl` to access the APIs

To check whether the APIs are running, we can run some of the following commands on the host machine.

To check what the port mapping is, we can either look at the `docker ps` command above, or find out the range from the state file:

```console
❯ cat $PWD/testing/materializer/tests/docker-materializer-data/materializer-state.json \
  | jq -c ".port_ranges.\"testnets/layer2/root/subnets/england/nodes/london\""
{"from":30300,"to":30400}
```

Probe CometBFT:
```bash
curl http://localhost:30357/status
```

Probe the Ethereum API:
```bash
curl -X POST \
           -H 'Content-Type: application/json' \
           -d '{"jsonrpc":"2.0","id":0,"method":"eth_chainId","params":[]}' \
           http://localhost:30345
```

Probe Fendermint Prometheus metrics:
```bash
curl http://localhost:30384/metrics
```

The ports get allocated from 30000 onward, 100 range to each node, so the last two digits resemble to internal ports:
* 8045 -> 30x45
* 26657 -> 30x57
* 9184 -> 30x84

#### Logs

For troubleshooting we can look at the logs, either by using `docker logs` and the container name, or for the `fendermint` container we can also access the logs:
* `less testing/materializer/tests/docker-materializer-data/testnets/layer2/root/nodes/brussels/fendermint/logs/fendermint.2024-03-11.log`
* `docker logs brussels-fendermint-955632`


#### Env vars

The node containers all get same env vars, which are written to the `static.env` and `dynamic.env` files and actuated by the entry point. If something doesn't look right, the files can be inspected, or the parsed configuration printed like this:

```console
❯ docker exec -it london-fendermint-05d823 bash
I have no name!@london-fendermint-05d823:~$ /opt/docker/docker-entry.sh "fendermint config" /opt/docker/static.env /opt/docker/dynamic.env
```


### Teardown

The following command removes all containers in the testnet:

```bash
cargo run -q -p fendermint_app -- \
  materializer \
    --data-dir $PWD/testing/materializer/tests/docker-materializer-data/ \
    teardown --testnet-id layer2
```

For `--testnet-id` pass the name of the manifest file, without the extension. All containers are tagged with the testnet ID.
