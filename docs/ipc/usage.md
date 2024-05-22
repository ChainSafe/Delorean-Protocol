# Using the IPC CLI

>💡 For background and setup information, make sure to start with the [README](/README.md).

## Key management
The `ipc-cli` has internally an EVM wallet that it uses to sign transactions and interact with IPC on behalf of specific addresses. Some of the features available for EVM addresses through the EVM are:
* Creating new Ethereum addresses
```bash
./bin/ipc-cli wallet new --wallet-type evm
```
```console
# Sample execution
./bin/ipc-cli wallet new --wallet-type evm
"0x406a7a1d002b71ece175cc7e067620ae5b58e9ec"
```

* Exporting a key stored in the IPC cli keystore.
```bash
./bin/ipc-cli wallet export --wallet-type evm --address <EVM-ADDRESS> > <OUTPUT_FILE>
```
```console
# Sample execution
./bin/ipc-cli wallet export --wallet-type evm --address 0x406a7a1d002b71ece175cc7e067620ae5b58e9ec -o /tmp/priv.key
exported new wallet with address 0x406a7a1d002b71ece175cc7e067620ae5b58e9ec in file "/tmp/priv.key"
```

* You can also export your private key encoded in base64 in a format that can be consumed by Fendermint by adding the `--fendermint` flag.
```bash
./bin/ipc-cli wallet export --wallet-type evm --address <EVM-ADDRESS>  --fendermint > <OUTPUT_FILE>
```

* Or hex encoded as expected by Ethereum tooling (like Metamask or hardhat).
```bash
./bin/ipc-cli wallet export --wallet-type evm --address <EVM-ADDRESS> -o <OUTPUT_FILE> --hex
```

* Importing a key from a file
```bash
./bin/ipc-cli wallet import --wallet-type evm --path=<INPUT_FILE_WITH_KEY>
```
```console
# Sample execution
$ ./bin/ipc-cli wallet import --wallet-type evm --path=~/tmp/wallet.key
imported wallet with address "0x406a7a1d002b71ece175cc7e067620ae5b58e9ec"
```

> 💡 The format expected to import new EVM keys is the following:
> ```
> {"address":<EVM-ADDRESS>,"private_key":<PRIVATE_KEY>}
> ```
> You can always create this file manually to import some address into the agent that you have exported from some other tool with an alternative format.

* Importing an identity directly from its private key
```bash
./bin/ipc-cli wallet import --wallet-type evm --private-key <PRIVATE_KEY>
```
```console
# Sample execution
$ ./bin/ipc-cli wallet import --wallet-type evm --private-key=0x405f50458008edd6e2eb2efc3bf34846db1d6689b89fe1a9f9ccfe7f6e301d8d
imported wallet with address "0x406a7a1d002b71ece175cc7e067620ae5b58e9ec"
```

* You can set a default key for your wallet so it is always the one used when the `--from` flag is not explicitly set
```bash
./bin/ipc-cli wallet set-default --address <EVM-ADDRESS> --wallet-type evm
```

* And check what is your current default key:
```bash
./bin/ipc-cli wallet get-default --wallet-type evm
```

* Check the hex encoded public key of your address with:
```bash
./bin/ipc-cli wallet pub-key --wallet-type evm --address=<EVM-address>
```

## Listing active subnets

As a sanity-check that we have joined the subnet successfully and that the subnet has been registered in IPC successfully can be performed through:

```bash
./bin/ipc-cli subnet list --subnet=<PARENT_SUBBNET_ID>
```
```console
# Example execution
$ ./bin/ipc-cli subnet list --subnet=/r31415926
/r31415926/t01003 - status: 0, collateral: 2 FIL, circ.supply: 0.0 FIL
```

This command only shows subnets that have been registered to the gateway, i.e. that have provided enough collateral to participate in the IPC protocol and haven't been killed. It is not an exhaustive list of all of the subnet actors deployed over the network.

## Joining a subnet and adding collateral

* To join a subnet with the `ipc-cli`
```bash
./bin/ipc-cli subnet join --subnet <subnet-id> --collateral <collateral_amount>
```
```console
# Example execution
$ ./bin/ipc-cli subnet join --subnet=/r314159/t410fh4ywg4wvxcjzz4vsja3uh4f53johc2lf5bpjo6i --collateral=1
```
This command specifies the subnet to join, the amount of collateral to provide and the public key of the `--from` address that is joining as a validator.

* And to stake more collateral as a validator:

```bash
./bin/ipc-cli subnet stake --subnet <subnet-id> --collateral <collateral_amount>
```
```console
# Example execution
$ ./bin/ipc-cli subnet stake --subnet=/r314159/t410fh4ywg4wvxcjzz4vsja3uh4f53johc2lf5bpjo6i --collateral=1
```

> 💡 Note that changes in collateral and the power table are not reflected immediately in the parent. They need to be confirmed in the execution of the next bottom-up checkpoint, so until this happen, even if there has been a change in collateral, you may not be the change immediately when running `ipc-cli subnet list`. This impacts any change to the collateral of validators, i.e. `stake`, `unstake` and `leave` commands. In order to inspect the changes to the power table that have been performed between two epochs you can use the following command:
> ```bash
> ./bin/ipc-cli checkpoint list-validator-changes --from-epoch=<START_EPOCH> --to-epoch=<END_EPOCH>
> ```

## Listing your balance in a subnet
In order to send messages in a subnet, you'll need to have funds in your subnt account. You can use the following command to list the balance of your wallets in a subnet:
```bash
./bin/ipc-cli wallet balances --wallet-type evm --subnet <subnet-id>
```
```console
# Example execution
$ ./bin/ipc-cli wallet balances --subnet=/r31415926/t4xwzbdu7z5sam6hc57xxwkctciuaz7oe5omipwbq
```

## Sending funds in a subnet

The agent provides a command to conveniently exchange funds between addresses of the same subnet. This can be achieved through the following command:
```bash
./bin/ipc-cli subnet send-value --subnet <subnet-id> [--from <from-addr>] --to <to-addr> <value>
```
```console
# Example execution
$ ./bin/ipc-cli subnet send-value --subnet /r31415926/t4xwzbdu7z5sam6hc57xxwkctciuaz7oe5omipwbq --to t1xbevqterae2tanmh2kaqksnoacflrv6w2dflq4i 10
```

## Sending funds between subnets

At the moment, the IPC agent only expose commands to perform the basic IPC interoperability primitives for cross-net communication, which is the exchange of FIL (the native token for IPC) between the same address of a subnet. Mainly:
- `fund`, which sends native token from one public key address, to the same public key address in the child.
- `release` that movesnative token from one account in a child subnet to its counter-part in the parent.

Complex behavior can be implemented using these primitives: sending value to a user in another subnet can be implemented a set of `release/fund` and `sendValue` operations. Calling  smart contract from one subnet to another works by providing funds to one account in the destination subnet, and then calling the contract. The `ipc-cli` doesn't currently include abstractions for this complex operations, but it will in the future. That being said, users can still leverage the `ipc-cli` or even the `IpcProvider` library to easily compose the basic primitives into complex functionality (in case you want to hack something cool and contribute to the project :) ).

>💡 All cross-net operations need to pay an additional cross-msg fee (apart from the gas cost of the message). This is reason why even if you sent `X FIL` you may see `X - fee FIL` arriving to you account at destination. This fee is used to reward subnet validators for their work committing the checkpoint that carries the message.

### Fund
Funding a subnet can be performed by using the following command:
```bash
./bin/ipc-cli cross-msg fund --subnet <subnet-id> [--from <from-addr>] [--to <to-addr>] <amount>
```
```console
# Example execution
$ ./bin/ipc-cli cross-msg fund --subnet /r31415926/t4xwzbdu7z5sam6hc57xxwkctciuaz7oe5omipwbq 100
```
This command includes the cross-net message into the next top-down proof-of-finality. Once the top-down finality is committed in the child, the message will be executed and you should see the funds in your account of the child subnet. If the `--to` is not set explicitly, the funds are send to the address of the `--from` in the subnet.

Alternatively, we can pass an additional parameter to send the funds to a specific address in the child subnet

```console
# Example execution
$ ./bin/ipc-cli cross-msg fund --subnet /r31415926/t4xwzbdu7z5sam6hc57xxwkctciuaz7oe5omipwbq --to=0x406a7a1d002b71ece175cc7e067620ae5b58e9ec 100
fund performed in epoch 1030279
```

The epoch were the message is performed can give you a sense of the time the message will take to be propagated. You can check the current finality in a subnet and wait for the finality height that includes your message to be committed.
```bash
./bin/ipc-cli cross-msg parent-finality --subnet <SUBNET_ID>
```

```console
# Example execution
$ ./bin/ipc-cli cross-msg parent-finality --subnet /r31415926/t4xwzbdu7z5sam6hc57xxwkctciuaz7oe5omipwbq
1030
```

>💡 Top-down proofs-of-finality is the underlying process used for IPC to propagate information from the parent to the child. Validators in the child subnet include information in every block in the child subnet about the height of the parent they agree to consider final. When this information is committed on-chain, changes into the validator set of the subnet, and the execution of top-down messages are correspondingly triggered.

* In order to list the top-down messages sent for a subnet from a parent network for a specific epoch, run the following command:
```bash
./bin/ipc-cli cross-msg list-topdown-msgs --subnet=<SUBNET_ID> --epoch=<EPOCH>

```

#### Funding subnet address in genesis
In order to fund your address in a child subnet genesis before it is bootstrapped, and include some funds on your address in the subnet in genesis, you can use the `pre-fund` command. This command can only be used before the subnet is bootsrapped and started. The inverse of this operation is `pre-release`, which allows you to recover some of these initial funds before the subnet starts:
```bash
./bin/ipc-cli cross-msg pre-fund --subnet <subnet-id> [--from <from-addr>] <amount>
```
```console
# Example execution
$ ./bin/ipc-cli cross-msg pre-fund --subnet=/r31415926/t4xwzbdu7z5sam6hc57xxwkctciuaz7oe5omipwbq 0.1
```

### Release
In order to release funds from a subnet, your account must hold enough funds inside it. Releasing funds to the parent subnet can be permformed with the following commnd:
```bash
./bin/ipc-cli cross-msg release --subnet <subnet-id> [--from <from-addr>] [--to <to-addr>] <amount>
```
```console
# Example execution
$ ./bin/ipc-cli cross-msg release --subnet=/r31415926/t4xwzbdu7z5sam6hc57xxwkctciuaz7oe5omipwbq 100
```
This command includes the cross-net message into a bottom-up checkpoint after the current epoch. Once the bottom-up checkpoint is committed in the parent, you should see the funds in your account in the parent. If the `--to` is not set explicitly, the funds are send to the address of the `--from` in the parent.

Alternatively, we can pass an additional parameter to release the funds to a specific address in the parent subnet

```console
# Example execution
$ ./bin/ipc-cli cross-msg release --subnet /r31415926/t4xwzbdu7z5sam6hc57xxwkctciuaz7oe5omipwbq --to=0x406a7a1d002b71ece175cc7e067620ae5b58e9ec 100
release performed in epoch 1030
```
As with top-down messages, you can get a sense of the time that your message will take to get to the parent by looking at the epoch in which your bottom-up message was triggered (the output of the command), and listing the latest bottom-up checkpoints to see how far it is from being propagated.

The propagation of a bottom-up checkpoint from a child subnet to its parent follows these stages:
* Validators in the child subnet populate the checkpoint, sign it, and agree on their validity. When validators have agreed on the validity of a checkpoint for a specific epoch, a new `QuorumReached` event is emitted in the child subnet. You can check if a checkpoint for certain epoch has already been signed by a majority of child validators through the following command: `./bin/ipc-cli checkpoint quorum-reached-events --from-epoch 600 --to-epoch 680 --subnet`.
```shell
# Sample execution
./bin/ipc-cli checkpoint quorum-reached-events --from-epoch 600 --to-epoch 680 --subnet /r314159/t410ffumhfeppdjixhkxtgagowxkdu77j7xz5aaa52vy
```

* Once validators have agree on the checkpoint to be submitted in the parent for a specific epoch, relayers need to pick up the checkpoint and submit it in the parent. The following commands can be used to determine what is the state of this submission:
  * Check if the address of a relayer has already submitted a checkpoint for execution in the parent for the latest checkpoint: `./bin/ipc-cli checkpoint has-submitted-bottomup-height --subnet <SUBNET_ID> --submitter <RELAYER_ADDR>`
  * Check the height of the latest checkpoint committed in the parent: `./bin/ipc-cli checkpoint last-bottomup-checkpoint-height --subnet <SUBNET_ID>`

Finally, the bundle of checkpoints and signatures populated and already signed by a child subnet for their submission to the parent on a window of heights can be checked through the command `./bin/ipc-cli checkpoint list-bottomup-bundle --subnet <SUBNET> --from-epoch <FROM_EPOCH> --to-epoch <TO_EPOCH>`

#### Releasing initial subnet balance
To recover some (or all) of the funds that were sent to a subnet through `pre-fund` to be included as genesis balance for your address, you can use the `pre-release` command as follows:
```bash
./bin/ipc-cli cross-msg pre-release --subnet <subnet-id> [--from <from-addr>] <amount>
```
```console
# Example execution
$ ./bin/ipc-cli cross-msg pre-release --subnet=/r31415926/t4xwzbdu7z5sam6hc57xxwkctciuaz7oe5omipwbq 0.1
```

## Running a relayer
IPC relies on the role of a specific type of peer on the network called the relayers that are responsible for submitting bottom-up checkpoints that have been finalized in a child subnet to its parent. This process is key for the commitment of child subnet checkpoints in the parent, and the execution of bottom-up cross-net messages. Without relayers, cross-net messages will only flow from top levels of the hierarchy to the bottom, but not the other way around.

* *session* Run the relayer process for your subnet using your default address by calling:
```bash
./bin/ipc-cli checkpoint relayer --subnet <SUBNET_ID>
```
* In order to run the relayer from a different address you can use the `--submitted` flag:
```bash
./bin/ipc-cli checkpoint relayer --subnet <SUBNET_ID> --submitter <RELAYER_ADDR>
```

Relayers are rewarded through cross-net messages fees for the timely submission of bottom-up checkpoints to the parent. In order to claim the checkpointing rewards collected for a subnet, the following command need to be run from the relayer address:
```bash
./bin/ipc-cli subnet claim --subnet=<SUBNET_ID> --reward
```

## Listing checkpoints from a subnet

Subnets are periodically committing checkpoints to their parent every `bottomup-check-period` (parameter defined when creating the subnet). If you want to inspect the information of a range of bottom-up checkpoints committed in the parent for a subnet, you can use the `checkpoint list-bottomup` command provided by the agent as follows:
```bash
./bin/ipc-cli checkpoint list-bottomup --from-epoch <range-start> --to-epoch <range-end> --subnet <subnet-id>
```
```console
# Example execution
$ ./bin/ipc-cli checkpoint list-bottomup --from-epoch 0 --to-epoch 100 --subnet /r31415926/t4xwzbdu7z5sam6hc57xxwkctciuaz7oe5omipwbq
epoch 0 - prev_check={"/":"bafy2bzacedkoa623kvi5gfis2yks7xxjl73vg7xwbojz4tpq63dd5jpfz757i"}, cross_msgs=null, child_checks=null
epoch 10 - prev_check={"/":"bafy2bzacecsatvda6lodrorh7y7foxjt3a2dexxx5jiyvtl7gimrrvywb7l5m"}, cross_msgs=null, child_checks=null
epoch 30 - prev_check={"/":"bafy2bzaceauzdx22hna4e4cqf55jqmd64a4fx72sxprzj72qhrwuxhdl7zexu"}, cross_msgs=null, child_checks=null
```
You can find the checkpoint where your cross-message was included by listing the checkpoints around the epoch where your message was sent.

## Leaving a subnet and releasing collateral

* To join a subnet with the `ipc-cli`
```bash
./bin/ipc-cli subnet join --subnet <subnet-id> --collateral <collateral_amount>
```
```console
# Example execution
$ ./bin/ipc-cli subnet join --subnet=/r314159/t410fh4ywg4wvxcjzz4vsja3uh4f53johc2lf5bpjo6i --collateral=1
```
This command specifies the subnet to join, the amount of collateral to provide and the public key of the `--from` address that is joining as a validator.

* To join a subnet and also include some initial balance for the validator in the subnet, you can add the `--initial-balance` flag with the balance to be included in genesis:
```bash
./bin/ipc-cli subnet join --subnet <subnet-id> --collateral <collateral_amount> --initial-balance <genesis-balance>
```
```console
# Example execution
$ ./bin/ipc-cli subnet join --subnet=/r314159/t410fh4ywg4wvxcjzz4vsja3uh4f53johc2lf5bpjo6i --collateral=1 \
    --initial-balance 0.5
```

* To leave a subnet, the following agent command can be used:
```bash
./bin/ipc-cli subnet leave --subnet <subnet-id>
```
```console
# Example execution
$ ./bin/ipc-cli subnet leave --subnet /r31415926/t4xwzbdu7z5sam6hc57xxwkctciuaz7oe5omipwbq
```
Leaving a subnet will release the collateral for the validator and remove all the validation rights from its account. This means that if you have a validator running in that subnet, its validation process will immediately terminate.


* Validators can also reduce their collateral in the subnet through `unstake`

```bash
./bin/ipc-cli subnet stake --subnet <subnet-id> --collateral <collateral_amount>
```
```console
# Example execution
$ ./bin/ipc-cli subnet stake --subnet=/r314159/t410fh4ywg4wvxcjzz4vsja3uh4f53johc2lf5bpjo6i --collateral=1
```

> 💡 Remember, as described in the joining and leaving collateral section, that changes to the validator set and their collateral are not reflected immediately. Validator changes between two epochs can be inspected through:
> ```bash
> ./bin/ipc-cli checkpoint list-validator-changes --from-epoch=<START_EPOCH> --to-epoch=<END_EPOCH>
> ```

* Once the reduction of collateral has been confirmed by the subnet, validators can claim their collateral back through:
```bash
./bin/ipc-cli subnet claim --subnet=/r314159/t410fh4ywg4wvxcjzz4vsja3uh4f53johc2lf5bpjo6i
```
