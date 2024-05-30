# Delorean Protocol

Encrypt To-The-Future with programmable key release conditions!

## Overview

The [DRAND protocol](https://drand.love/) and [tlock](https://github.com/drand/tlock) allow for practical encryption to the future. A plaintext can is encrypted using Identity Based Encryption (IBE) such that the decryption key is produced when a threshold of DRAND operators sign a chosen message. The message is chosen to be a block height in the future so you can be certain that when the network reaches that height the decryption key will be automatically generated and made public by the network.

Delorean protocol extends this idea to allow programmable conditions for decryption key release while maintaining the same (and in some cases better) security guarantees. Users can deploy Solidity smart contracts that encode the conditions under which the network operators must generate the decryption key in order to keep the network progressing. Some examples might be:

- Fund Raising
    - Key is released once the contract raises some amount of funds
- Hush Money
    - Key is released if the contract does not receive a periodic payment
- Dead Man Switch
    - Key is released the contract it doesn't receive a 'heartbeat' transaction from a preauthrized account for a number of blocks

Using the CLI it is possible to encrypt files which can be published to IPFS or Filecoin allowing for a fully distributed encrypted data access control.

## Design

Delorean is implemented as an [IPC Subnet](https://docs.ipc.space/) allowing it to be easily bootstrapped and have access to assets from parent chains (e.g. FIL). It uses CometBFT for consensus and a fork of Fendermint for execution which communicate via ABCI.

The feature of CometBFT that makes the protocol possible is the [vote extensions](https://docs.cosmos.network/main/build/abci/vote-extensions) functionality added in ABCI v0.38. In CometBFT validators vote on finalized blocks. Vote extensions allow the execution layer to enforce additional data to be included for votes to be valid. In Delorean the additional data is a BLS signature over the next tag in the queue (more on this later). If this signature is not included then the vote is invalid and cannot be used to finalize the block. The result is that the liveness of the chain becomes coupled to the release of these signed tags and inherits the same guarantees.

Deloran adds a new actor to the FVM that stores the queue of tags and allows contracts in the FEVM to pay gas to add new tags to the queue. Once a tag is added to the queue the validators must include a signature over it in their next vote or else they are committing a consensus violation. The block proposer combines all of the signatures from the votes into an aggregate and includes an additional transaction which writes it back to the state. This aggregate signature is a valid decryption key.

A tag is defined as:

```
tag = SHA256(contractAddress || arbitaryData)
```

It is defined this way so someone else cannot write another contract which causes your key to be released. The tag (and hence decryption key itself) is tied to the address of the contract that manages it.

The tag used in combination with the validator public keys when encrypting data as defined in [^tlock]. This takes place fully-offchain and without any communication with the validators as follows:

```
ciphertext = encrypt(message, aggValidatorKey, tag)
```

A decryption key is derived from a tag as:

```
key = aggregate([sign(tag, key) for key in validatorKeys])
```

and this is what the Delorean protocol produces when the contract triggers the call to release the key.

## Usage Flow

Creating conditionally decryptable data with Delorean can be done as follows:

1. Create the Solidity contract encoding the key release conditions. This should call `DeloreanAPI.enqueueTag(<tag>)` only when the conditions are met.

2. Deploy this contract to the Delorean subnet chain and obtain its contract address

3. Encrypt the data locally using the Delorean CLI `cat data.txt | delorean encrypt <contract-address> > encrypted.txt`
    - Under the hood this retrieves the tag and validator aggregate BLS public keys by making RPC calls to the Delorean client

4. (optional) Upload the data to a public network such as IPFS or Filecoin

To attempt to decrypt data run the following

1. `cat ./encrypted.txt | delorean decrypt <contract-address> > decrypted.txt`

    - This will look in the state and see if the decryption key for this data has been released. If so it will decrypt it otherwise it will error


## Security considerations

The security of the protocol relies on the following. Where possible these are compared with DRAND/tlock

- The 2/3 of the network validators have not colluded to produce decryption keys in secret
    - This is the same assumption as tlock although here it is slightly better. In tlock because the tags are block heights and hence predictable if the operators ever collude they can derive all possible future keys. In Delorean they can only derive all keys for known tags.

- The protocol depends on the underlying security of the Threshold BLS encryption scheme of [^tlock]

- Liveness of the key release inherits the same liveness guarantees as CometBFT. That is less than 1/3 of the total weight of the validator set is malicious or faulty


## References

- [^tlock]: [tlock: Practical Timelock Encryption from Threshold BLS](https://eprint.iacr.org/2023/189)