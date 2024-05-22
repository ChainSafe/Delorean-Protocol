# IPC Spec - CometBFT

This document explains the relationship between Fendermint and CometBFT.

# What is CometBFT?

[CometBFT](https://cometbft.com/) is a *generic* blockchain engine using the Tendermint consensus. What this means for us has been argued in the original [solution concept](https://docs.google.com/document/d/1cFoTdoRuYgxmWJia6K-b5vmEj-4MvyHCNvShZpyconU) that introduced it (still as Tendermint Core at the time) for use for IPC.

By *generic* we mean that CometBFT doesn’t say anything about the format and content of the *transactions* and the *ledger* of the replicated state machine; that is completely within the jurisdiction of the *application*, which is Fendermint.

The two communicate over the Application BlockChain Interface (ABCI). We are currently using [version 0.37](https://docs.cometbft.com/v0.37/) with [ABCI v1](https://docs.cometbft.com/v0.37/spec/abci/) ; version 0.38 moves to [ABCI v2](https://docs.cometbft.com/v0.38/spec/abci/abci++_methods), which will unlock new capabilities.

To get an idea of how little CometBFT says about what the transactions and the ledger should look like, have a look at the [kvstore example](https://docs.cometbft.com/v0.37/guides/go-built-in) in the guide; there are no signatures, the transactions are literally just strings formatted as `"key=value"`.  Check out [these steps](https://github.com/consensus-shipyard/ipc/blob/main/docs/fendermint/tendermint.md) to see how to get started with CometBFT by running the built-in `kvstore` application.

A really good overview of the Tendermint consensus lifecycle in CometBFT is provided by the following diagram:

![https://docs.tendermint.com/v0.34/assets/img/tm-transaction-flow.258ca020.png](https://docs.tendermint.com/v0.34/assets/img/tm-transaction-flow.258ca020.png)

# What is Fendermint?

Fendermint implements a ledger using the ABCI interface, driven by CometBFT. It contains the definition of transactions and queries, and uses the FVM to execute messages and store the data as IPLD.

See the [IPS Spec - Executions](https://www.notion.so/IPS-Spec-Executions-ebf13d833d6845ec9c11b59bd514fcda?pvs=21) and [IPC Spec - Snapshots](https://www.notion.so/IPC-Spec-Snapshots-edcbd2716d4e48e3bd3fe8d39a22fdb2?pvs=21) to get a detailed overview of how the ABCI methods are implemented, and the [Architecture diagram](https://github.com/consensus-shipyard/ipc/blob/main/docs/fendermint/architecture.md) for the component layout.

## Common Misconceptions

There were some questions we should clarify, as they are not immediately clear for someone not familiar with how CometBFT works.

### How many CometBFT instances per Fendermint?

There is a 1-to-1 relationship between CometBFT and Fendermint. CometBFT feeds the blocks to Fendermint for execution. Fendermint maintains the application state with deterministic execution of transactions. Fendermint is not a standalone application, with CometBFT as some kind of a front-end, like the Ethereum API facade. It is just an execution engine.

### Can Fendermint instances collude and lie to CometBFT?

This assumes that Fendermint instances communicate between each other in a Byzantine way, present their version of truth to CometBFT, and then the question is what happens if Fenderrmint isn’t majority honest. No, it doesn’t work that way.

Fendermint instances do not run consensus between themselves, that is precisely the job of CometBFT. Each CometBFT is associated with a single dedicated and trusted Fendermint instance that it owns, and communicates with other CometBFT instances about the results of their respective transaction executions.

### How many subnets does Fendermint run?

Each CometBFT+Fendermint chain is a specific subnet. To run a subnet, the operators must launch a separate blockchain.

### Does Fendermint call CometBFT through ABCI?

No, the ABCI application does not normally make RPC calls to CometBFT, the communication is strictly initiated by CometBFT, the application acts as the server reacting to events happening in consensus.

Fendermint itself calls CometBFT via JSON-RPC to enquire about the history of the power table, and the status of the chain syncing, but that has to happen outside the ABCI cycle.

# Process Model

CometBFT is written in Go, while Fendermint is written in Rust. Had Fendermint been written in Go, we would have the opportunity to compile a `fendermint` binary that runs CometBFT with an in-process implementation of the Fendermint ABCI ledger. This is how [Cosmos SDK](https://docs.cosmos.network/) applications work. In our case, however, we have to run two distinct processes: `cometbft` and `fendermint`, with the communication happening over TCP. They can be deployed on the same machine, or separately.

In Docker they are two distinct containers, with `cometbft` being one of the official images. This has the advantage that there is no need to audit it, because we haven’t changed anything.

💡 Sometimes the question comes up whether the `fendermint` process should somehow own `cometbft` and try to hide the fact that it even exists to simplify its operation and deployment - in this regard it would make it more similar to the experience of native Cosmos SDK apps written in Go, which are executed in the CometBFT process itself.

It is worth keeping in mind in this context that [securely deploying](https://docs.cometbft.com/v0.37/core/validators#setting-up-a-validator) CometBFT in production is a task left for engineering. Fendermint doesn’t contain any tools or documentation to do this for the user.

For this reason it is worth highlighting the need for operators to familiarise themselves with CometBFT. Trying to hide CometBFT as an implementation detail would risk obscuring it, and making it more difficult for them to find expert advice on operating their blockchain.

When in need for inspiration, one can look at the [Penumbra deployments](https://github.com/penumbra-zone/penumbra/tree/v0.74.0/deployments), which have a similar split between a Rust application and the CometBFT engine.
