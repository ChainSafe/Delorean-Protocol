![](./Delorean-Docs/assets/banner.png)

![Static Badge](https://img.shields.io/badge/build_for-HackFS_2024-green)
![GitHub License](https://img.shields.io/github/license/BadBoiLabs/Delorean-Protocol)


An IPC subnet for programmable encryption to-the-future.

Our entry for the EthGlobal HackFS 2024 Hackathon

## Judging Considerations

Delorean builds upon the IPC subnet boilerplace provided by Protocol Labs

The work conducted during the hackathon can be viewed in [this diff](https://github.com/BadBoiLabs/Delorean-Protocol/compare/72a51df1dc5ee06474772406171ebcabdc205b69..main)

This includes:

- Updating the runtime to support ABCI v0.38
- [Developing a custom FVM actor (Delorean actor)](https://github.com/BadBoiLabs/Delorean-Protocol/blob/main/fendermint/actors/cetf/src/actor.rs)
- [Solidity contracts to interface with actor](https://github.com/BadBoiLabs/Delorean-Protocol/blob/main/contracts/src/delorean/DeloreanAPI.sol)
- [Modifications to runtime to implement the Delorean protocol](https://github.com/BadBoiLabs/Delorean-Protocol/blob/main/fendermint/app/src/app.rs)
    - Extend CometBFT votes with signatures
    - Ensure votes have valid signatures
    - Have block producer commit aggregate signature to actor state
- [Delorean CLI](https://github.com/BadBoiLabs/Delorean-Protocol/tree/main/fendermint/testing/delorean-cli)
    - encrypt/decrypt

## Overview

Practical encryption to the future can be done using a blockchain in combination with witness encryption. A plaintext can is encrypted such that the decryption key is produced when a threshold of validators sign a chosen message. This is used by protocols such as tlock[^tlock] and McFLY[^mcfly].

The message is chosen to be a block height in the future. With particular blockchains such as [DRAND](https://drand.love) you can be certain that when the network reaches that height the validators will sign it and the decryption key will be automatically generated and made public by the network. The limitation of this is it only supports encryption to the future. No additional constraints can be placed on key generation.

Delorean protocol extends this idea to allow programmable conditions for decryption key release while maintaining the same (and in some cases better) security guarantees. Users can deploy Solidity smart contracts that encode the conditions under which the network operators must generate a decryption key.Some examples might be:

- Fund Raising
    - Key is released once the contract raises some amount of funds
- Hush Money
    - Key is released if the contract does not receive a periodic payment
- Dead Man Switch
    - Key is released the contract it doesn't receive a 'heartbeat' transaction from a preauthrized account for a number of blocks
- Encrypted Transactions / Encrypted Mempool
- Filecoin Deal integration
    - Key is released once a particular Filecoin deal is discontinued
- Price Oracle integration
    - Key is released if some asset reaches a given price

Being an IPC subnet Delorean is a fully fledged EVM chain that can also message other subnets in the wider network. This means conditions could be encoded that rely on other chains (e.g Filecoin deals).

We also developed a CLI that makes it easy to encrypt and decrypt files with keys tied to deployed condition contracts. Encryption and decryption takes place fully off-chain and does not require communication with the validators.

## Architecture / How it's made

Delorean is implemented as an [IPC Subnet](https://docs.ipc.space/) allowing it to be easily bootstrapped and have access to assets from parent chains (e.g. FIL). It uses CometBFT for consensus and a modified version of the FVM for execution which communicates with the consensus layer via ABCI.

The feature of CometBFT that makes the protocol possible is the [vote extension](https://docs.cosmos.network/main/build/abci/vote-extensions) feature added in ABCI v0.38. In CometBFT validators vote on blocks to be finalized. Vote extensions allow the execution layer to enforce additional data to be included for votes to be valid. In Delorean the additional data is a BLS signature over the next tag in the queue (more detail on this later). If this signature is not included then the vote is invalid and cannot be used to finalize the block. The result is that the liveness of the chain becomes coupled to the release of these signed tags and inherits the same guarantees.

### Delorean Actor

Deloran adds a new actor to the FVM that stores the queue of tags and allows contracts in the FEVM to pay gas to enqueue new tags. Once a tag is added to the queue the validators MUST include a signature over it in their next vote or else they are committing a consensus violation. The block proposer combines all of the signatures from the votes into an aggregate and includes an additional transaction which writes it back to the actor state. This aggregate signature is a valid decryption key for data encrypted to this tag!

A tag is defined as:

```
tag = SHA256(contractAddress || arbitaryData)
```

It is defined this way so someone else cannot write another contract which causes your key to be released. The tag (and hence decryption key itself) is tied to the address of the contract that manages it.

### Cryptography

We use the threshold BLS Identify Based Encryption algorithm of [Gailly, Melissaris and Yolan Romailler (2023)](^tlock) for encryption and decryption.

Encryption is done using the tag used in combination with the validator public keys. This takes place fully-offchain and without any communication with the validators as follows:

```
ciphertext = encrypt(message, aggValidatorKey, tag)
```

A decryption key is derived from a tag as:

```
key = aggregate([sign(tag, key) for key in validatorKeys])
```

and this is what the Delorean protocol produces after the contract triggers the call to release the key.

> [!NOTE]  
> We are using the non-threshold variant of the algorithm for this prototype so all validators must sign. A threshold version could be implemented by having the validators perform a key generation ceremony or by using a modified protocol such as [McFLY -  Döttling et al (2023)](mcfly)

## Usage Flow

Creating conditionally decryptable data with Delorean can be done as follows:

1. Create the Solidity contract encoding the key release conditions. This should call `DeloreanAPI.enqueueTag(<tag>)` only when the conditions are met.

2. Deploy this contract to the Delorean subnet chain and obtain its contract address

3. Encrypt the data locally using the Delorean CLI

```shell
echo "secret message" | delorean encrypt <contract-address> > encrypted.txt
```

Under the hood this retrieves the tag from the contract and validator aggregate BLS public keys by making RPC calls to the Delorean client

4. (optional) Upload the data to a public network such as IPFS or Filecoin

5. To attempt to decrypt data run the following

```shell
cat ./encrypted.txt | delorean decrypt <contract-address> > decrypted.txt
```

This will look in the state and see if the decryption key for this data has been released. If so it will decrypt it otherwise it will error.

## Running the Demo

### Prerequisites

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

Run `make` in the root of the repo before proceeding

### Steps

The demo runs against a standalone network (not a subnet) with 4 validators.

To start the testnet run the following:

```shell
cd fendermint/testing/delorean-cli/
cargo make setup-cetf && cargo make node-1-setup && cargo make node-2-setup  && cargo make node-3-setup
```

> ![INFO]
>If you want to add the network to MetaMask the RPC is `http://localhost:8545` and the Chain ID is `2555887744985227`

Wait a few minutes to build the node docker images and to set up the network. It is comprised of a number of docker containers which can be viewed in the docker desktop application or by running `docker ps`

Install the CLI with

```shell
cargo install --path .
```

Then setup our demo with the following:

```shell
cargo make register-bls-keys  
cargo make deploy-demo-contract
```

Set env vars for the deployed contract address to use in future commands

```shell
export CONTRACT_ADDRESS="0x....."
export DELORIAN_SECRET_KEY="./test-data/keys/volvo.sk"
```

### Encrypt

The encrypt command takes a stream on std-in and encrypts it. Lets pipe a message to have it encrypted

```shell
echo 'Where we are going, we dont need centralized key registries!' | delorean-cli encrypt $CONTRACT_ADDRESS -o encrypted.txt 
```

Take a look at the encrypted output by running `cat encrypted.txt`. It uses the [age](https://github.com/FiloSottile/age) encryption specification to encrypt large plaintexts.

### Trigger Key Generation

Now to trigger the decryption key generation.

Deposit at least 88 FIL to the contract to enable the key release conditions

```shell
```

Then call the `releaseKey` method 

```shell
delorean-cli call-release-keys $CONTRACT_ADDRESS
```

### Decrypt

The decrypt command can now be used to retrieve the generated key from the store and decrypt our data

```shell
cat ./encrypted.txt | delorean-cli decrypt $CONTRACT_ADDRESS
```

## Security Considerations

The security of the protocol relies on the following. Where possible these are compared with DRAND/tlock

- The 2/3 of the network validators have not colluded to produce decryption keys in secret
    - This is the same assumption as tlock although here it is slightly better. In tlock because the tags are block heights and hence predictable if the operators ever collude they can derive all possible future keys. In Delorean they can only derive all keys for known tags.

- The protocol depends on the underlying security of the Threshold BLS encryption scheme of [^tlock]

- Liveness of the key release inherits the same liveness guarantees as CometBFT. That is less than 1/3 of the total weight of the validator set is malicious or faulty

## Future Work

Delorean becomes particularly powerful when paired with ZKPs. This would allow the published ciphertext to have accompanying proofs about its content (e.g. that it will decrypt to a private key corresponding to a public key).

That way you could be certain if the keys are released after some funding goal is met that the data will actually decrypt. Even more useful would be if this can be extended to arbitrary ZK proofs about the encrypted data itself. This would allow all kinds of interesting applications (e.g. encrypted transactions).

## References

- [^tlock]: [tlock: Practical Timelock Encryption from Threshold BLS](https://eprint.iacr.org/2023/189)
- [^mcfly]: [McFly: Verifiable Encryption to the Future Made Practical](https://eprint.iacr.org/2022/433)
