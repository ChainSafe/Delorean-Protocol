// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
//! A constant running process that fetch or listener to parent state

mod syncer;
mod tendermint;

use crate::proxy::ParentQueryProxy;
use crate::sync::syncer::LotusParentSyncer;
use crate::sync::tendermint::TendermintAwareSyncer;
use crate::voting::VoteTally;
use crate::{CachedFinalityProvider, Config, IPCParentFinality, ParentFinalityProvider, Toggle};
use anyhow::anyhow;
use async_stm::atomically;
use ethers::utils::hex;
use ipc_ipld_resolver::ValidatorKey;
use std::sync::Arc;
use std::time::Duration;

use fendermint_vm_genesis::{Power, Validator};

pub use syncer::fetch_topdown_events;

/// Query the parent finality from the block chain state.
///
/// It returns `None` from queries until the ledger has been initialized.
pub trait ParentFinalityStateQuery {
    /// Get the latest committed finality from the state
    fn get_latest_committed_finality(&self) -> anyhow::Result<Option<IPCParentFinality>>;
    /// Get the current committee voting powers.
    fn get_power_table(&self) -> anyhow::Result<Option<Vec<Validator<Power>>>>;
}

/// Queries the starting finality for polling. First checks the committed finality, if none, that
/// means the chain has just started, then query from the parent to get the genesis epoch.
async fn query_starting_finality<T, P>(
    query: &Arc<T>,
    parent_client: &Arc<P>,
) -> anyhow::Result<IPCParentFinality>
where
    T: ParentFinalityStateQuery + Send + Sync + 'static,
    P: ParentQueryProxy + Send + Sync + 'static,
{
    loop {
        let mut finality = match query.get_latest_committed_finality() {
            Ok(Some(finality)) => finality,
            Ok(None) => {
                tracing::debug!("app not ready for query yet");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
            Err(e) => {
                tracing::warn!(error = e.to_string(), "cannot get committed finality");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };
        tracing::info!(finality = finality.to_string(), "latest finality committed");

        // this means there are no previous committed finality yet, we fetch from parent to get
        // the genesis epoch of the current subnet and its corresponding block hash.
        if finality.height == 0 {
            let genesis_epoch = parent_client.get_genesis_epoch().await?;
            tracing::debug!(genesis_epoch = genesis_epoch, "obtained genesis epoch");
            let r = parent_client.get_block_hash(genesis_epoch).await?;
            tracing::debug!(
                block_hash = hex::encode(&r.block_hash),
                "obtained genesis block hash",
            );

            finality = IPCParentFinality {
                height: genesis_epoch,
                block_hash: r.block_hash,
            };
            tracing::info!(
                genesis_finality = finality.to_string(),
                "no previous finality committed, fetched from genesis epoch"
            );
        }

        return Ok(finality);
    }
}

/// Queries the starting finality for polling. First checks the committed finality, if none, that
/// means the chain has just started, then query from the parent to get the genesis epoch.
async fn query_starting_comittee<T>(query: &Arc<T>) -> anyhow::Result<Vec<Validator<Power>>>
where
    T: ParentFinalityStateQuery + Send + Sync + 'static,
{
    loop {
        match query.get_power_table() {
            Ok(Some(power_table)) => return Ok(power_table),
            Ok(None) => {
                tracing::debug!("app not ready for query yet");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
            Err(e) => {
                tracing::warn!(error = e.to_string(), "cannot get comittee");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        }
    }
}

/// Start the polling parent syncer in the background
pub async fn launch_polling_syncer<T, C, P>(
    query: T,
    config: Config,
    view_provider: Arc<Toggle<CachedFinalityProvider<P>>>,
    vote_tally: VoteTally,
    parent_client: Arc<P>,
    tendermint_client: C,
) -> anyhow::Result<()>
where
    T: ParentFinalityStateQuery + Send + Sync + 'static,
    C: tendermint_rpc::Client + Send + Sync + 'static,
    P: ParentQueryProxy + Send + Sync + 'static,
{
    if !view_provider.is_enabled() {
        return Err(anyhow!("provider not enabled, enable to run syncer"));
    }

    let query = Arc::new(query);
    let finality = query_starting_finality(&query, &parent_client).await?;

    let power_table = query_starting_comittee(&query).await?;
    let power_table = power_table
        .into_iter()
        .map(|v| {
            let vk = ValidatorKey::from(v.public_key.0);
            let w = v.power.0;
            (vk, w)
        })
        .collect::<Vec<_>>();

    atomically(|| {
        view_provider.set_new_finality(finality.clone(), None)?;
        vote_tally.set_finalized(finality.height, finality.block_hash.clone())?;
        vote_tally.set_power_table(power_table.clone())?;
        Ok(())
    })
    .await;

    tracing::info!(
        finality = finality.to_string(),
        "launching parent syncer with last committed finality"
    );

    start_syncing(
        config,
        view_provider,
        vote_tally,
        parent_client,
        query,
        tendermint_client,
    );

    Ok(())
}

/// Start the parent finality listener in the background
fn start_syncing<T, C, P>(
    config: Config,
    view_provider: Arc<Toggle<CachedFinalityProvider<P>>>,
    vote_tally: VoteTally,
    parent_proxy: Arc<P>,
    query: Arc<T>,
    tendermint_client: C,
) where
    T: ParentFinalityStateQuery + Send + Sync + 'static,
    C: tendermint_rpc::Client + Send + Sync + 'static,
    P: ParentQueryProxy + Send + Sync + 'static,
{
    let mut interval = tokio::time::interval(config.polling_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    tokio::spawn(async move {
        let lotus_syncer =
            LotusParentSyncer::new(config, parent_proxy, view_provider, vote_tally, query)
                .expect("");

        let mut tendermint_syncer = TendermintAwareSyncer::new(lotus_syncer, tendermint_client);

        loop {
            interval.tick().await;

            if let Err(e) = tendermint_syncer.sync().await {
                tracing::error!(error = e.to_string(), "sync with parent encountered error");
            }
        }
    });
}
