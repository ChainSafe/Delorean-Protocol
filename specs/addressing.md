# Subnet Addressing

## IPC Actors addressing

### Filecoin adressing schemes

When the contracts described below (Gateway, Registry, Subnet) are deployed to a Filecoin network (e.g. Mainnet, Calibration), `f0`, `f2` and `f410` addresses will be assigned to them. The latter is equivalent to an Ethereum hex address.
Detailed explanation of Filecoin addressing scheme is available in [the documentation](https://docs.filecoin.io/smart-contracts/filecoin-evm-runtime/address-types).

This also applies to IPC L2+ networks since IPC inherits the addressing model, the Init actor mechanics, and the EVM runtime and the Ethereum Address Manager from Filecoin.

### Gateway Actor and Registry Actor addressing

Gateway and Registry actors are EVM smart contracts, so they inherit the addresses specified above.

### Subnet Actor addressing

Subnet Actor is an EVM smart contract deployed separately for each child Subnet and registered in the parent's Gateway.
It gets assigned the address types stated above.
The creation of Subnet Actor is performed via the Registry actor (which acts like a factory).
The Ethereum address acquired by the Subnet Actor is determined by the semantics of the EVM `CREATE` opcode.
It's worth noting that the caller address seen by `CREATE` will be that of the Registry diamond contract.
One can use this fact to counterfactually predict subnet actor deployment addresses ahead of time, although being aware of the non-determinism present in us using `CREATE` instead of `CREATE2` for the time being.

## Subnet address

IPC subnets are uniquely identified by their [SubnetId](https://github.com/consensus-shipyard/ipc/blob/main/contracts/src/structs/Subnet.sol#L9) which consists of

- `uint64 root` - a [Chain ID](https://chainlist.org/?search=filecoin&testnets=true) of the root subnet. Eg. all subnets anchored to Filecoin Mainnet have `root` equal to `314`
- `address[] route`- the array of addresses down the IPC hierarchy.

[`SubnetIDHelper`](https://github.com/consensus-shipyard/ipc/blob/main/contracts/src/lib/SubnetIDHelper.sol) contains utility functions to create and operate on `SubnetID`s

### String representation

The string representation of subnet address equals:

- prefix `/r` indicating root chainID directly followed by the value of `root` Chain ID (eg. `/r314` for Filecoin Mainnet).
- concatenated with the f410 addresses of the subnet actors top-to-bottom in the hierarchy, separated by `/` as a divider.

Example 1: the string representation of the Filecoin Mainnet itself is `/r314` as this is a root, not anchored to any parent.

Example 2: the string representation of an L3 subnet anchored to Filecoin Calibration (`chainID` equal `314159`) could be `/r314159/t410fgalav7yo342zbem3kkqhx4l5d43d3iyswlpwkby/t410fixm5mqenkfm2g6msjt2chs36cxaa7ka745xo2jq`

where `t410fgalav7yo342zbem3kkqhx4l5d43d3iyswlpwkby` and `t410fixm5mqenkfm2g6msjt2chs36cxaa7ka745xo2jq` are addresses of L2 and L3 subnet respectively.

> Note that the `t` prefix denotes a test network. `f` denotes mainnet.

### Binary representation

`SubnetID` is serialised using `keccak256(abi.encode(SubnetID))` ([utility function](https://github.com/consensus-shipyard/ipc/blob/main/contracts/src/lib/SubnetIDHelper.sol#L58)).

This hash is used as the way to verify equality of 2 `SubnetID` s ([utility function](https://github.com/consensus-shipyard/ipc/blob/main/contracts/src/lib/SubnetIDHelper.sol#L89)), store mapping of hash to the Subnet object etc.

## IPC Address

[IPCAddress](https://github.com/consensus-shipyard/ipc/blob/main/contracts/src/structs/Subnet.sol#L149) contains both `SubnetID` and `FvmAddress` to uniquely identify an actor (EOA or smart contract) existing within the IPC hierarchy.
