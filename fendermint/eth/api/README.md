# Ethereum API Facade

The `fendermint_eth_api` crate implements some of the [Ethereum JSON-RPC](https://ethereum.org/en/developers/docs/apis/json-rpc) methods. The up-to-date list of which methods are implemented can be gleaned from the [API registration](./src/apis/mod.rs).

The API is tested for basic type lineup during the `make e2e` tests via the [ethers example](./examples/ethers.rs).

The relevant specification is [FIP-55](https://github.com/filecoin-project/FIPs/blob/master/FIPS/fip-0055.md).