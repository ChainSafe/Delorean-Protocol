# IPC Diamond

The IPC Solidity Actors are implemented using the Diamond pattern, but the current implementation
is not compatible with [EIP-2535](https://eips.ethereum.org/EIPS/eip-2535) standard:

All the diamonds are upgradable and implement `IDiamondCut` and `IDiamondLoupe` interfaces.

## Code Layout

1. The SubnetActor facets are stored in the `subnet` directory. The Gateway facets are stored in the `gateway` directory.
2. `GatewayDiamond.sol` and `SubnetActorDiamond.sol` are diamond contracts.
3. Libraries are stored in the `lib` directory. They contain functionality that can't fit in a facet or should be shared by multiple facets.
4. `lib/LibSubnetActor.sol` and `lib/LibGatewayActorStorage` implement `AppStorage` pattern.
5. A custom `lib/ReentrancyGuard.sol` is used because the original OpenZeppelin's `ReentrancyGuard` contract doesn't support the Diamond pattern.

## Implementation Base

The IPC diamond code is based on the [diamond-1-hardhat](https://github.com/mudgen/diamond-1-hardhat/tree/main/contracts) reference implementation.

## Storage

The implementation uses the `AppStorage` pattern in facets and `Diamond Storage` in libraries.
`GatewayActorStorage` and `SubnetActorStorage` are used within the `AppStorage` pattern.
To be compatible with `ApStorage` and to be able to apply it, we are using the `LibReentrancyGuard` contract.

## Getting Selectors

Because diamonds contain mappings of function selectors to facet addresses, we have to know function selectors before deploying.
To do that, we use the `get_selectors` function from a script in Python.

## References

-   [Introduction to EIP-2535 Diamonds](https://eip2535diamonds.substack.com/p/introduction-to-the-diamond-standard)
-   [ERC-2535: Diamonds, Multi-Facet Proxy](https://eips.ethereum.org/EIPS/eip-2535#facets-state-variables-and-diamond-storage)
-   [Understanding Diamonds on Ethereum](https://dev.to/mudgen/understanding-diamonds-on-ethereum-1fb)
