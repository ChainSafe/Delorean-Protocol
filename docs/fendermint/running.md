# Running Fendermint

The commands are all executed by the `fendermint` binary, which is produced from the `fendermint_app` crate,
so we have many ways to run the program:
* `fendermint <args>`, after running `cargo install --path fendermint/app`
* `./target/release/fendermint <args>`, after running `cargo build --release`
* `cargo run -p fendermint_app --release -- <args>`

The same is also available for step-by-step execution in the [Milestone-1 demo](./demos/milestone-1/README.md).

> TIP: If something goes wrong with the RPC commands, try to run them with `fendermint --log-level debug rpc ...` to see the JSON-RPC requests and responses.

## Genesis

The first step we need to do is create a Genesis file which we'll pass to Tendermint,
which will pass it to Fendermint through ABCI. This Genesis file will be in JSON format,
as that is the convention with Tendermint, but we could also encode it in CBOR if we wanted.

Ostensibly the Genesis content could be coming from the parent chain itself, so the child
subnet participants don't have to go through the manual steps below, but we still have these
options to start a standalone network.

In the following sections we will create a Genesis file for a network named `test`. Start by creating a directory to hold the files:

```shell
mkdir test-network
```

If you are running in test network, define the network using env variable.
```shell
export FM_NETWORK=test
```

### Create a new Genesis file

First, create a new `genesis.json` file devoid of accounts and validators. The `--base-fee` here is completely arbitrary.
The `--power-scale` value of `0` means we'll grant 1 voting power per 1 FIL; to use more precision, we can set it to `3`
to use milliFIL for example.

```shell
cargo run -p fendermint_app --release -- \
  genesis --genesis-file test-network/genesis.json \
  new \
  --chain-name test \
  --base-fee 1000 \
  --timestamp 1680101412 \
  --power-scale 0
```

We can check what the contents look like:

```console
$ cat test-network/genesis.json
{
  "timestamp": 1680101412,
  "chain_name": "test",
  "network_version": 18,
  "base_fee": "1000",
  "power_scale": 0,
  "validators": [],
  "accounts": []
}
```

### Create some keys

Next, let's create some cryptographic key pairs we want want to use either for accounts or validators at Genesis.

```shell
mkdir test-network/keys
for NAME in alice bob charlie dave; do
  cargo run -p fendermint_app --release -- key gen --out-dir test-network/keys --name $NAME;
done
```

Check that the keys have been created:

```console
$ ls test-network/keys
alice.pk  alice.sk  bob.pk  bob.sk  charlie.pk  charlie.sk  dave.pk  dave.sk

$ cat test-network/keys/alice.pk
Ak5Juk793ZAg/7Ojj4bzOmIFGpwLhET1vg2ROihUJFkq
```

If you want to use existing ethereum private key, perform the follwoing:

```shell
cargo run -p fendermint_app --release -- key eth-to-fendermint --secret-key <path to private key> --name eth --out-dir test-network/keys
```

### Add accounts to the Genesis file

Add one of the keys we created to the Genesis file as a stand-alone account:

```shell
 cargo run -p fendermint_app --release -- \
        genesis --genesis-file test-network/genesis.json \
        add-account --public-key test-network/keys/alice.pk --balance 10
```

If your key is from ethereum, add a `kind` flag to indicate that:

```shell
 cargo run -p fendermint_app --release -- \
        genesis --genesis-file test-network/genesis.json \
        add-account --public-key test-network/keys/alice.pk --balance 10 --kind ethereum
```

Check that the balance is correct:

```console
$ cat test-network/genesis.json | jq .accounts
[
  {
    "meta": {
      "Account": {
        "owner": "f1jqqlnr5b56rnmc34ywp7p7i2lg37ty23s2bmg4y"
      }
    },
    "balance": "10000000000000000000"
  }
]
```

The `owner` we see is an `f1` type address with the hash of the public key. Technically it's an `Address` type,
but it has to be one based on a public key, otherwise we would not be able to validate signatures later.

Let's add an example of the other possible account type, a multi-sig account:

```shell
cargo run -p fendermint_app --release -- \
        genesis --genesis-file test-network/genesis.json \
        add-multisig --public-key test-network/keys/bob.pk --public-key test-network/keys/charlie.pk --public-key test-network/keys/dave.pk \
          --threshold 2 --vesting-start 0 --vesting-duration 1000000 --balance 30
```

Check that all three of the signatories have been added:

```console
$ cat test-network/genesis.json | jq .accounts[1]
{
  "meta": {
    "Multisig": {
      "signers": [
        "f1kgtzp5nuob3gdccagivcgns7e25be2c2rqozilq",
        "f1bvdmcbct6vwoh3rvkhoth2fe66p6prpbsziqbfi",
        "f1hgeqjtadqmyabmy2kij2smn5jiiud75kva6bzny"
      ],
      "threshold": 2,
      "vesting_duration": 1000000,
      "vesting_start": 0
    }
  },
  "balance": "30000000000000000000"
}
```

### Add validators to the Genesis file

Finally, let's add one validator to the Genesis, with a monopoly on voting power, so we can run a standalone node:

```shell
cargo run -p fendermint_app --release -- \
      genesis --genesis-file test-network/genesis.json \
      add-validator --public-key test-network/keys/bob.pk --power 1;
```

The value of power doesn't matter in this case, as `bob` is our only validator. It's value is expressed in tokens,
ie. FIL, and will be serialized in atto, hence the 18 zeroes.

Check the result:

```console
$ cat test-network/genesis.json | jq .validators
[
  {
    "public_key": "BCImfwVC/LeFJN9bB612aCtjbCYWuilf2SorSUXez/QEy8cVKNuvTU/EOTibo3hIyOQslvSouzIpR24h1kkqCSI=",
    "power": "1000000000000000000"
  },
]
```

The public key was spliced in as it was, in base64 format, which is how it would appear in Tendermint's
own genesis file format. Note that here we don't have the option to use `Address`, because we have to return
these as actual `PublicKey` types to Tendermint through ABCI, not as a hash of a key.

### (Optional) Add ipc to the Genesis file

If you need ipc related function, let's add the subnet info to the Genesis with deployed subnet id: /r31415926

```shell
cargo run -p fendermint_app --release -- \
      genesis --genesis-file test-network/genesis.json \
      ipc \
      gateway --subnet-id /r31415926 \
      --bottom-up-check-period 10 \
      --msg-fee 1 --majority-percentage 65
```
Check the result:
```console
$ cat test-network/genesis.json | jq .ipc
{
  "gateway": {
    "subnet_id": "/r31415926",
    "bottom_up_check_period": 10,
    "top_down_check_period": 10,
    "msg_fee": "1",
    "majority_percentage": 65
  }
}
```

### Configure CometBFT

First, follow the instructions in [getting started with CometBFT](./tendermint.md) to install the binary,
then initialize a genesis file for CometBFT at `~/.cometbft`.

```shell
rm -rf ~/.cometbft
cometbft init
```

The logs show that it created keys and a genesis file:

```console
I[2023-03-29|09:58:06.324] Found private validator                      module=main keyFile=/home/aakoshh/.cometbft/config/priv_validator_key.json stateFile=/home/aakoshh/.cometbft/data/priv_validator_state.json
I[2023-03-29|09:58:06.324] Found node key                               module=main path=/home/aakoshh/.cometbft/config/node_key.json
I[2023-03-29|09:58:06.324] Found genesis file                           module=main path=/home/aakoshh/.cometbft/config/genesis.json
```

#### Convert the Genesis file

We don't want to use the random values created by CometBFT; instead we need to use some CLI commands to convert the artifacts
file we created earlier to the format CometBFT accepts. Start with the genesis file:

```shell
mv ~/.cometbft/config/genesis.json ~/.cometbft/config/genesis.json.orig
cargo run -p fendermint_app --release -- \
  genesis --genesis-file test-network/genesis.json \
  into-tendermint --out ~/.cometbft/config/genesis.json
```

Check the contents of the created Comet BFT Genesis file:

<details>
  <summary>~/.cometbft/config/genesis.json</summary>

```console
$ cat ~/.cometbft/config/genesis.json
{
  "genesis_time": "2023-03-29T14:50:12Z",
  "chain_id": "test",
  "initial_height": "1",
  "consensus_params": {
    "block": {
      "max_bytes": "22020096",
      "max_gas": "-1",
      "time_iota_ms": "1000"
    },
    "evidence": {
      "max_age_num_blocks": "100000",
      "max_age_duration": "172800000000000",
      "max_bytes": "1048576"
    },
    "validator": {
      "pub_key_types": [
        "secp256k1"
      ]
    }
  },
  "validators": [],
  "app_hash": "",
  "app_state": {
    "accounts": [
      {
        "balance": "10000000000000000000",
        "meta": {
          "Account": {
            "owner": "f1jqqlnr5b56rnmc34ywp7p7i2lg37ty23s2bmg4y"
          }
        }
      },
      {
        "balance": "30000000000000000000",
        "meta": {
          "Multisig": {
            "signers": [
              "f1kgtzp5nuob3gdccagivcgns7e25be2c2rqozilq",
              "f1bvdmcbct6vwoh3rvkhoth2fe66p6prpbsziqbfi",
              "f1hgeqjtadqmyabmy2kij2smn5jiiud75kva6bzny"
            ],
            "threshold": 2,
            "vesting_duration": 1000000,
            "vesting_start": 0
          }
        }
      }
    ],
    "base_fee": "1000",
    "chain_name": "test",
    "network_version": 18,
    "timestamp": 1680101412,
    "validators": [
      {
        "power": 1,
        "public_key": "BCImfwVC/LeFJN9bB612aCtjbCYWuilf2SorSUXez/QEy8cVKNuvTU/EOTibo3hIyOQslvSouzIpR24h1kkqCSI="
      }
    ]
  }
}
```

</details>

We can see that our original `genesis.json` has been made part of CometBFT's version under `app_state`,
and that the top level `validators` are empty, to be filled out by the application during the `init_chain` ABCI call.


#### Convert the private key

By default CometBFT uses Ed25519 validator keys, but in theory it can use anything that looks like a key.

We can run the following command to replace the default `priv_validator_key.json` file with one based on
one of the validators we created.

```shell
mv ~/.cometbft/config/priv_validator_key.json ~/.cometbft/config/priv_validator_key.json.orig
cargo run -p fendermint_app --release -- \
  key into-tendermint --secret-key test-network/keys/bob.sk --out ~/.cometbft/config/priv_validator_key.json
```

See if it looks reasonable:

<details>
<summary>~/.cometbft/config/priv_validator_key.json</summary>

```console
$ cat ~/.cometbft/config/priv_validator_key.json
{
  "address": "66FA0CFB373BD737DBFC7CE70BEF994DD42A3812",
  "priv_key": {
    "type": "tendermint/PrivKeySecp256k1",
    "value": "04Gsfaw4RHZ5hTbXO/3hz2N567tz5E1yxChM1ZrEi1E="
  },
  "pub_key": {
    "type": "tendermint/PubKeySecp256k1",
    "value": "AiImfwVC/LeFJN9bB612aCtjbCYWuilf2SorSUXez/QE"
  }
}
$ cat test-network/keys/bob.pk
AiImfwVC/LeFJN9bB612aCtjbCYWuilf2SorSUXez/QE
```
</details>

## Run processes

The Fendermint Application and CometBFT will run as separate processes.

### Run the Application

Now we are ready to start our Fendermint Application, which will listen to requests coming from Tendermint
through the ABCI interface.

First, let's make sure we have put all the necessary files in an easy to remember place under `~/.fendermint`.

```shell
mkdir -p ~/.fendermint/data
cp -r ./fendermint/app/config ~/.fendermint/config
```

We will need the actor bundle to load. We can configure its location via environment variables, but the default
configuration will look for it at `~/.fendermint/bundle.car`, so we might as well put it there.

```shell
make actor-bundle
cp ./builtin-actors/output/bundle.car ~/.fendermint/bundle.car
cp ./actors/output/custom_actors_bundle.car ~/.fendermint/custom_actors_bundle.car
```

Now, start the application.

```shell
cargo run -p fendermint_app --release -- run
```

It is important to use the `--release` option, otherwise it will take too long to load the Wasm actor modules and
CometBFT will issue a timeout (by default we have 3 seconds to execute requests).

With the default `--log-level info` we can see that the application is listening:

```console
2023-03-29T09:17:28.548878Z  INFO fendermint::cmd: reading configuration path="/home/aakoshh/.fendermint/config"
2023-03-29T09:17:28.549700Z  INFO fendermint::cmd::run: opening database path="/home/aakoshh/.fendermint/data/rocksdb"
2023-03-29T09:17:28.879916Z  INFO tower_abci::server: starting ABCI server addr="127.0.0.1:26658"
2023-03-29T09:17:28.880023Z  INFO tower_abci::server: bound tcp listener local_addr=127.0.0.1:26658
```

If we need to restart the application from scratch, we can do so by erasing all RocksDB state:

```shell
rm -rf ~/.fendermint/data/rocksdb
```

### Run CometBFT

CometBFT can be configured via `~/.cometbft/config/config.toml`; see the default settings [here](https://docs.cometbft.com/v0.37/core/configuration).

Now we are ready to start CometBFT and let it connect to the Fendermint Application.

```shell
cometbft start
```

If we need to restart the application from scratch, we can erase all CometBFT state like so:

```shell
cometbft unsafe-reset-all
```

If all goes well, we will see block created in the Fendermint Application log as well the CometBFT log:

<details>
  <summary>Application log</summary>

```console
$ rm -rf ~/.fendermint/data/rocksdb && cargo run -p fendermint_app --release -- --log-level debug run
...
2023-05-19T09:13:45.400896Z DEBUG tower_abci::v037::server: new request request=Info(Info { version: "0.37.1", block_version: 11, p2p_version: 8, abci_version: "1.0.0" })
...
2023-05-19T09:13:45.401018Z DEBUG tower_abci::v037::server: flushing response response=Ok(Info(Info { data: "fendermint", version: "0.1.0", app_version: 0, last_block_height: block::Height(0), last_block_app_hash: AppHash(0171A0E402203AAAC8F10B0E837FDF2546C98BF164972B07B49196E25322711E3C4807CF8AD8) }))
2023-05-19T09:13:45.401262Z DEBUG tower_abci::v037::server: new request request=InitChain(...)
...
2023-05-19T09:13:54.062109Z DEBUG tower_abci::v037::server: new request request=PrepareProposal(...)
...
2023-05-19T09:13:54.083246Z DEBUG tower_abci::v037::server: new request request=ProcessProposal(ProcessProposal { ..., height: block::Height(3), ... })
...
2023-05-19T09:13:54.105797Z DEBUG fendermint_app::app: begin block height=3
2023-05-19T09:13:54.105922Z DEBUG fendermint_app::app: initialized exec state
...
2023-05-19T09:13:54.110007Z DEBUG fendermint_app::app: commit state state_root="bafy2bzacebh4fbl6rv7tlxxf2zsxqifjr424tkykwmgffqaho6mvr6hy7dq42" timestamp=1684487633
```
</details>


<details>
  <summary>CometBFT log</summary>

```console
$ cometbft unsafe-reset-all && cometbft start
...
I[2023-05-19|10:13:45.449] Completed ABCI Handshake - CometBFT and App are synced module=consensus appHeight=0 appHash=0171A0E402203AAAC8F10B0E837FDF2546C98BF164972B07B49196E25322711E3C4807CF8AD8
I[2023-05-19|10:13:45.449] Version info                                 module=main tendermint_version=0.37.1 abci=1.0.0 block=11 p2p=8 commit_hash=2af25aea6
I[2023-05-19|10:13:45.449] This node is a validator                     module=consensus addr=1202F4D1C5ACCC8219E2973394CBD06FD1F33B5A pubKey=PubKeySecp256k1{02DBBA09ABF7888AA63D75534A8A0CD79209B0E549DFB3FDE015FC61069D1C7232}
...
I[2023-05-19|10:13:54.061] Timed out                                    module=consensus dur=984.901925ms height=3 round=0 step=RoundStepNewHeight
I[2023-05-19|10:13:54.079] received proposal                            module=consensus proposal="Proposal{3/0 (08CCBA6EDC7B6E77022D98A1BA528F34D2BDFFB94FE02DD36A3ECB873C321E07:1:ADBA4ABBE9A6, -1) 28842808EA1D @ 2023-05-19T09:13:54.072518233Z}"
I[2023-05-19|10:13:54.082] received complete proposal block             module=consensus height=3 hash=08CCBA6EDC7B6E77022D98A1BA528F34D2BDFFB94FE02DD36A3ECB873C321E07
I[2023-05-19|10:13:54.098] finalizing commit of block                   module=consensus height=3 hash=08CCBA6EDC7B6E77022D98A1BA528F34D2BDFFB94FE02DD36A3ECB873C321E07 root=0171A0E402204FC2857E8D7F35DEE5D6657820A98F35C9AB0AB30C52C007779958F8F8F8E1CD num_txs=0
I[2023-05-19|10:13:54.106] executed block                               module=state height=3 num_valid_txs=0 num_invalid_txs=0
I[2023-05-19|10:13:54.110] committed state                              module=state height=3 num_txs=0 app_hash=0171A0E402204FC2857E8D7F35DEE5D6657820A98F35C9AB0AB30C52C007779958F8F8F8E1CD
I[2023-05-19|10:13:54.116] indexed block exents                         module=txindex height=3
...
```
</details>

Note that the first block execution is very slow because we have to load the Wasm engine, as indicated by the first proposal having a timeout,
but after that the blocks come in fast, one per second.

### Run ETH API
If we want to use `evm` related API, such as running `fendermint/eth/api/examples/ethers.rs`, we need to start ETH API process.

The ETH RPC api runs on top of cometbft. Make sure you have cometbft running properly. The architecture is as follows:
```
+---------------------------+
| Node                      |
|                           |
|   ------------------      |
|   | fendermint run |      |
|   |                |      |
|   | :26658         |      |
|   ------------------      |
|     ^                     |
|     |                     |
|   ------------------      |
|   | cometbft       |      |
|   |                |      |
|   | :26657         |      |
|   ------------------      |
|     |                     |
| :26657                    |
+---------------------------+
  ^
  |
-----------------------------
| Ethereum API              |
|                           |
|   +-------------------+   |
|   | fendermint eth run|   |
|   |                   |   |
|   | :8545             |   |
|   +-------------------+   |
|     |                     |
| :8545                     |
-----------------------------
```
To start the ethereum RPC api with:
```
cargo run -p fendermint_app --release -- eth run
```
We will see:
<details>
  <summary>ETH API log</summary>

```console
2023-07-20T12:30:48.385026Z  INFO fendermint::cmd: reading configuration path="/home/admin/.fendermint/config"
2023-07-20T12:30:48.435387Z  INFO fendermint_eth_api: bound Ethereum API listen_addr=127.0.0.1:8545
```

</details>

We can try query the chain id by:
```shell
curl -X POST -i   -H 'Content-Type: application/json'   -d '{"jsonrpc":"2.0","id":0,"method":"eth_chainId","params":[]}'   http://localhost:8545
```

### Access Metrics

By default `fendermint` has Prometheus metrics enabled (with more to be added) and available at http://localhost:9184/metrics.

## Query the state

The Fendermint binary has some commands to support querying state. Behind the scenes it uses the `tendermint_rpc` crate to talk
to the CometBFT JSON-RPC endpoints, which forward the requests to the Application through ABCI.

Assuming both processes have been started, see if we can query the state of one of our actors. For this we need the actor address,
which we saw in the `genesis.json` file earlier.

To make it easier to reuse these addresses, let's store them in variables:

```shell
ALICE_ADDR=$(cargo run -p fendermint_app --release -- key address --public-key test-network/keys/alice.pk)
BOB_ADDR=$(cargo run -p fendermint_app --release -- key address --public-key test-network/keys/bob.pk)
```

```shell
cargo run -p fendermint_app --release -- \
  rpc query actor-state --address $ALICE_ADDR
```

The state is printed to STDOUT as JSON:

```console
$ echo $ALICE_ADDR
f1i2izmkzef5q6udtdooeujzfsuieybxzl2yer5ey
$ cargo run -p fendermint_app --release --   rpc query actor-state --address $ALICE_ADDR
{
  "id": 100,
  "state": {
    "balance": "10000000000000000000",
    "code": "bafk2bzacealdyp7dmpc6eir65qhuh2hgv7onmv53rzzyp5futafmjjlxrt6fg",
    "delegated_address": null,
    "sequence": 0,
    "state": "bafy2bzaceas2zajrutdp7ugb6w2lpmow3z3klr3gzqimxtuz22tkkqallfch4"
  }
}
```

What we see here is the general [ActorState](https://github.com/filecoin-project/builtin-actors/blob/v10.0.0/actors/account/src/state.rs) which contains the balance, the nonce, the Wasm code CID, and the state root hash of the
actual actor implementation, which in this case is an `Account` actor.

We can retrieve the raw state of the account with the `ipld` command by using the `state.state` from above as the `--cid` argument:

```shell
cargo run -p fendermint_app --release -- \
        rpc query ipld --cid bafy2bzaceas2zajrutdp7ugb6w2lpmow3z3klr3gzqimxtuz22tkkqallfch4
```

The binary contents are printed with Base64 encoding, which we could pipe to a file. It would be more useful to run this query
programmatically and parse it to the appropriate data structure from [builtin-actors](https://github.com/filecoin-project/builtin-actors).

```console
gVUBRpGWKyQvYeoOY3OJROSyogmA3ys=
```

## Transfer tokens

The simplest transaction we can do is to transfer tokens from one account to another.

For example we can send 0.1 tokens from Alice to Bob:

```shell
cargo run -p fendermint_app --release -- \
  rpc transfer --secret-key test-network/keys/alice.sk --to $BOB_ADDR --sequence 0 --value 0.1 --chain-name test
```

Note that we are using `--sequence 0` because this is the first transaction we make using Alice's key.

The `transfer` command waits for the commit results of the transaction:

```console
$ cargo run -p fendermint_app --release -- rpc transfer --secret-key test-network/keys/alice.sk --to $BOB_ADDR --sequence 0 --value 0.1 --chain-name test
    Finished dev [unoptimized + debuginfo] target(s) in 0.40s
     Running `target/debug/fendermint rpc transfer --secret-key test-network/keys/alice.sk --to f1kgtzp5nuob3gdccagivcgns7e25be2c2rqozilq --sequence 0 --value 0.1`
{
  "response": {
    "check_tx": {
      "code": 0,
      "codespace": "",
      "data": null,
      "events": [],
      "gas_used": "0",
      "gas_wanted": "10000000000",
      "info": "",
      "log": "",
      "mempool_error": "",
      "priority": "0",
      "sender": "f1jqqlnr5b56rnmc34ywp7p7i2lg37ty23s2bmg4y"
    },
    "deliver_tx": {
      "code": 0,
      "codespace": "",
      "data": null,
      "events": [],
      "gas_used": "1124863",
      "gas_wanted": "10000000000",
      "info": "",
      "log": ""
    },
    "hash": "01828E0A350445ED3E8028D045EE99B5547B6834DB7296B799B95707EB546EC2",
    "height": "46"
  },
  "return_data": null
}
```

The `code: 0` parts indicate that both check and delivery were successful. Let's check the resulting states:

```console
$ cargo run -p fendermint_app --release -- rpc query actor-state --address $BOB_ADDR | jq .state.balance
"1000"

$ cargo run -p fendermint_app --release -- rpc query actor-state --address $ALICE_ADDR | jq "{balance: .state.balance, sequence: .state.sequence}"
{
  "balance": "999999999999999000",
  "sequence": 1
}
```

Great, Alice's nonce was correctly increased as well.


## Create FEVM Contract

When we want to deploy a smart contract to the FVM, the currently supported way is by deploying EVM contracts to FEVM.

First, we need the `solc` compiler to produce the binaries we want deployed; take a look at the [test contracts](https://github.com/filecoin-project/builtin-actors/tree/next/actors/evm/tests/contracts) in the `builtin-actors` repo for example.

Say we want to deploy the `SimpleCoin` contract from that directory.

```shell
CONTRACT=../builtin-actors/actors/evm/tests/contracts/SimpleCoin.bin
cargo run -p fendermint_app --release -- \
  rpc fevm --secret-key test-network/keys/alice.sk --sequence 1  --chain-name test \
    create --contract $CONTRACT
```

Note that now we are using `--sequence 1` because this is the second transaction sent by Alice.

The output shows what addresses have been assigned to the created contract,
which we can use to call the contract, namely by copying the `delegated_address`.

```console
$ CREATE=$(cargo run -p fendermint_app --release -- \
        rpc fevm --secret-key test-network/keys/alice.sk --sequence 1 --chain-name test \
          create --contract $CONTRACT)

$ echo $CREATE | jq .return_data
{
    "actor_address": "f0105",
    "actor_id": 105,
    "actor_id_as_eth_address": "ff00000000000000000000000000000000000069",
    "delegated_address": "f410f7do6trb2wkh6vwj6wpg5oxpgbipmj6btcc3kipq",
    "eth_address": "f8dde9c43ab28fead93eb3cdd75de60a1ec4f833",
    "robust_address": "f2yapgav7dqzifnry3ccqfg3xdzek73zd7rjvwsci"
}

$ DELEGATED_ADDR=$(echo $CREATE | jq -r .return_data.delegated_address)
```

## Invoke FEVM Contract

Now that we have a contract deployed, we can call it. The arguments in the followign example are taken from [fvm-bench](https://github.com/filecoin-project/fvm-bench). We need to increment the `--sequence` again.

```console
$ cargo run -p fendermint_app --release -- \
              rpc fevm --secret-key test-network/keys/alice.sk --sequence 2 \
                invoke --contract $DELEGATED_ADDR  \
                       --method f8b2cb4f --method-args 000000000000000000000000ff00000000000000000000000000000000000064 \
          | jq .return_data
"0000000000000000000000000000000000000000000000000000000000002710"
```

If we look at the [method signatures](https://github.com/filecoin-project/builtin-actors/blob/v10.0.0/actors/evm/tests/contracts/SimpleCoin.signatures#L2) we can see that this is calling [getBalance](https://github.com/filecoin-project/builtin-actors/blob/v10.0.0/actors/evm/tests/contracts/simplecoin.sol#L28), and indeed if we decode `2710` from hexadecimal to decimal, we get the `10000` balance the owner should have.

To avoid having to come up with ABI encoded arguments in hexadecimal format, we can use the RPC client in combination with [ethers](https://docs.rs/crate/ethers/latest) excellent `abigen` functionality.

Here's an [example](../fendermint/rpc/examples/simplecoin.rs) of doing that with the [SimpleCoin](https://github.com/filecoin-project/builtin-actors/blob/v10.0.0/actors/evm/tests/contracts/simplecoin.sol) contract.

```console
$ cargo run -p fendermint_rpc --release --example simplecoin -- --secret-key test-network/keys/alice.sk --verbose
2023-05-19T10:18:47.234878Z DEBUG fendermint_rpc::client: Using HTTP client to submit request to: http://127.0.0.1:26657/
...
2023-05-19T10:18:47.727563Z  INFO simplecoin: contract deployed contract_address="f410fvbmxiqdn6svyo5oubfbzxsorkvydcb5ecmlbwma" actor_id=107
...
2023-05-19T10:18:48.805085Z  INFO simplecoin: owner balance balance="10000" owner_eth_addr="ff00000000000000000000000000000000000064"
```

Note that the script figures out the Alice's nonce on its own, so we don't have to pass it in. It also has an example of running an EVM view method (which is read-only) either as as a distributed read-transaction (which is included on the chain and costs gas) or a query anwered by our node without involving the blockchain. Both have their uses, depending on our level of trust.

## Deploy IPC child subnet

### Crate genesis from parent
Fendermint includes a command to automatically create the genesis file for an IPC child subnet according to the information for the subnet available in its parent. Here's an example of the generation of a genesis file for a subnet that has already been bootstrapped in the parent.
```shell
cargo run -p fendermint_app -- \
    --network=test \
    genesis --genesis-file test-network/genesis.json \
    ipc from-parent --subnet-id <CHILD_SUBNET_ID> -p <PARENT_ENDPOINT> \
    --parent-gateway <PARENT_GATEWAY_CONTRACT> \
    --parent-registry <PARENT_REGISTRY_CONTRACT>
```

Here's a sample execution of the command for an already bootstrapped subnet in `/r314159`:
```shell
cargo run -p fendermint_app -- \
    --network=test \
    genesis --genesis-file test-network/genesis.json \
    ipc from-parent \
    --subnet-id /r314159/t410fdoh27lsddz4my2v3e77qnxdp5vsjxkdfokc7sti \
    -p https://api.calibration.node.glif.io/rpc/v1 \
    --parent-gateway 0x56948d2CFaa2EF355B8C08Ac925202db212146D1 \
    --parent-registry 0x6A4884D2B6A597792dC68014D4B7C117cca5668e```

Leading to the following genesis file:
```console
{
  "chain_name": "/r314159/t410fdoh27lsddz4my2v3e77qnxdp5vsjxkdfokc7sti",
  "timestamp": 1007560,
  "network_version": 18,
  "base_fee": "1000",
  "power_scale": 3,
  "validators": [
    {
      "public_key": "BDOFw7mriml8177GymI8vdD+oSk+i0ZN+CWxBOtYpEzI76zGo0grhmuF7N9zS11O9UlXN96zSGJc5qNVNhQtKVU=",
      "power": "1000000000000000000"
    }
  ],
  "accounts": [],
  "ipc": {
    "gateway": {
      "subnet_id": "/r314159/t410fdoh27lsddz4my2v3e77qnxdp5vsjxkdfokc7sti",
      "bottom_up_check_period": 30,
      "msg_fee": "1000000000000",
      "majority_percentage": 60,
      "min_collateral": "1000000000000000000",
      "active_validators_limit": 100
    }
}
```
