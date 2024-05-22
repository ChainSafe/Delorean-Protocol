# Architecture

The following diagrams show preliminary vision for the collaboration between the Filecoin rootnet and the subnets,
some of which can be implemented using Fendermint:

![Architecture](images/IPC%20with%20Tendermint%20Core.jpg)

The components in a nutshell:
* __Lotus rootnet__: Lotus has to support IPC to act as the rootnet for its child subnets. There are competing proposals with regards to this being in the form of mostly user-deployed smart contracts, or built-in (privileged) capabilities.
* __Tendermint Core__: acts as the generic SMR in one of the subnets, talking to all other Tendermint instances in that subnet. It runs separate from the Application, which is completely in our control, and talks to it via ABCI++.
* __Application__: This is where we implement the IPC ledger logic, the transaction handling, using the FVM/FEVM. We implement the ABCI++ interface to be compatible with Tendermint. Other than that we delegate to reusable smart contracts, for example to produce checkpoints. We rely on ABCI to pass us the headers to be signed, and we can use the ledger to gather signatures for checkpoints.
* __Parent View__: The Application might observe the parent subnet consensus state directly for the purpose of voting. The goal here is to deal with the fact that Lotus might roll back a message, so we can’t execute a top-down message as soon as it appears, we have to wait for it to be embedded. We can make sure of this either by a) including a light client of the parent in the child ledger or b) using the voting process and let validators take a peek at the other chain.
* __IPLD Resolver__: A separate process (or a [library](https://github.com/consensus-shipyard/ipc-ipld-resolver)) that the Application can contact to resolve CIDs and store them in the IPLD Store. It maintains connection with other nodes in the parent/child subnets, or perhaps even beyond.
* __IPLD Store__: A common store available for read/write for both the FVM and the IPLD Resolver. There might be a separate instance for each subnet, or one larger serving the needs of all subnets of an operator.
* __FVM__: This is our execution layer. Subnets can use FEVM if they want.
* __Relayer__: The role of relayers is to shovel messages between parent and child subnets. They have to follow both the parent and the child consensus, subscribe to events, re-package the messages in the appropriate formats and resend them. How they are incentivized to do so is an open-ended question. They should be trustless. Both subnets can have an entirely different block structure and consensus, and it’s only the relayers that understand both, by being purposefully constructed to act between certain combinations.

## ABCI++

We want make use of the ABCI++ interface to get more control over the voting process by implementing the new `PrepareProposal` and `ProcessProposal` methods. These are close to be [released](https://github.com/tendermint/tendermint/issues/9053) in the upcoming `v0.37` of Tendermint Core.

The best place to look up the details of the ABCI++ spec is currently at https://github.com/tendermint/tendermint/tree/v0.37.0-rc2/spec/abci

Note that the spec has previously been under the `main` branch but not any more, and that it changed recently to only contain the above two extra methods, but not _vote extensions_ for the new `FinalizeBlock` method, which was supposed to replace `BeginBlock`, `DeliverTx`, `EndBlock` and I think `Commit`.

The reason we want to be able to control voting is to evaludate the CIDs contained in blocks for data availability, before they are committed for execution. We can do this by simply not voting on any proposal that contains CIDs _for execution_ that are unavailable on the node of the validator. To make them available, we'll use a solution similar to [NC-Max](https://eprint.iacr.org/2020/1101) to propose CIDs _for resolution_ and inclusion in future blocks, thus moving data dissemination out of the critical path of consensus.
