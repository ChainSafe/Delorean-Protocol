# Integration test with The Graph

## Run test
```bash
cargo make setup     # Start CometBFT, Fendermint and ETH API
cargo make test      # Run Graph integration test
cargo make teardown
```

This test is derived from: https://docs.hedera.com/hedera/tutorials/smart-contracts/deploy-a-subgraph-using-the-graph-and-json-rpc

Reference for docker setup for subgraph: https://github.com/graphprotocol/graph-node/blob/master/docker/README.md
