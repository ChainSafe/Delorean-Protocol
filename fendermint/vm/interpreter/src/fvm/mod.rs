// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use std::path::PathBuf;

mod broadcast;
mod check;
mod checkpoint;
mod exec;
mod externs;
mod genesis;
mod query;
pub mod state;
pub mod store;
pub mod upgrades;
mod cetfkernel;

#[cfg(any(test, feature = "bundle"))]
pub mod bundle;
pub(crate) mod topdown;

pub use check::FvmCheckRet;
pub use checkpoint::PowerUpdates;
pub use exec::FvmApplyRet;
use fendermint_crypto::{PublicKey, SecretKey};
use fendermint_eth_hardhat::Hardhat;
pub use fendermint_vm_message::query::FvmQuery;
use fvm_ipld_blockstore::Blockstore;
pub use genesis::FvmGenesisOutput;
pub use query::FvmQueryRet;
use tendermint_rpc::Client;

pub use self::broadcast::Broadcaster;
use self::{state::ipc::GatewayCaller, upgrades::UpgradeScheduler};

pub type FvmMessage = fvm_shared::message::Message;

#[derive(Clone)]
pub struct ValidatorContext<C> {
    /// The secret key the validator uses to produce blocks.
    secret_key: SecretKey,
    /// The public key identifying the validator (corresponds to the secret key.)
    public_key: PublicKey,
    /// Used to broadcast transactions. It might use a different secret key for
    /// signing transactions than the validator's block producing key.
    broadcaster: Broadcaster<C>,
}

impl<C> ValidatorContext<C> {
    pub fn new(secret_key: SecretKey, broadcaster: Broadcaster<C>) -> Self {
        // Derive the public keys so it's available to check whether this node is a validator at any point in time.
        let public_key = secret_key.public_key();
        Self {
            secret_key,
            public_key,
            broadcaster,
        }
    }
}

/// Interpreter working on already verified unsigned messages.
#[derive(Clone)]
pub struct FvmMessageInterpreter<DB, C>
where
    DB: Blockstore + 'static + Clone,
{
    contracts: Hardhat,
    /// Tendermint client for querying the RPC.
    client: C,
    /// If this is a validator node, this should be the key we can use to sign transactions.
    validator_ctx: Option<ValidatorContext<C>>,
    /// Overestimation rate applied to gas to ensure that the
    /// message goes through in the gas estimation.
    gas_overestimation_rate: f64,
    /// Gas search step increase used to find the optimal gas limit.
    /// It determines how fine-grained we want the gas estimation to be.
    gas_search_step: f64,
    /// Indicate whether transactions should be fully executed during the checks performed
    /// when they are added to the mempool, or just the most basic ones are performed.
    exec_in_check: bool,
    /// Indicate whether the chain metadata should be pushed into the ledger.
    push_chain_meta: bool,
    gateway: GatewayCaller<DB>,
    /// Upgrade scheduler stores all the upgrades to be executed at given heights.
    upgrade_scheduler: UpgradeScheduler<DB>,
}

impl<DB, C> FvmMessageInterpreter<DB, C>
where
    DB: Blockstore + 'static + Clone,
{
    pub fn new(
        client: C,
        validator_ctx: Option<ValidatorContext<C>>,
        contracts_dir: PathBuf,
        gas_overestimation_rate: f64,
        gas_search_step: f64,
        exec_in_check: bool,
        upgrade_scheduler: UpgradeScheduler<DB>,
    ) -> Self {
        Self {
            client,
            validator_ctx,
            contracts: Hardhat::new(contracts_dir),
            gas_overestimation_rate,
            gas_search_step,
            exec_in_check,
            push_chain_meta: true,
            gateway: GatewayCaller::default(),
            upgrade_scheduler,
        }
    }

    pub fn with_push_chain_meta(mut self, push_chain_meta: bool) -> Self {
        self.push_chain_meta = push_chain_meta;
        self
    }
}

impl<DB, C> FvmMessageInterpreter<DB, C>
where
    DB: fvm_ipld_blockstore::Blockstore + 'static + Clone,
    C: Client + Sync,
{
    /// Indicate that the node is syncing with the rest of the network and hasn't caught up with the tip yet.
    async fn syncing(&self) -> bool {
        match self.client.status().await {
            Ok(status) => status.sync_info.catching_up,
            Err(e) => {
                // CometBFT often takes a long time to boot, e.g. while it's replaying blocks it won't
                // respond to JSON-RPC calls. Let's treat this as an indication that we are syncing.
                tracing::warn!(error =? e, "failed to get CometBFT sync status");
                true
            }
        }
    }
}
