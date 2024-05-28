// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use anyhow::Context;
use async_trait::async_trait;
use cid::Cid;
use ethers::types::spoof::State;
use fendermint_actor_cetf::Tag;

use fendermint_vm_actor_interface::cetf::{CETFSYSCALL_ACTOR_ADDR, CETFSYSCALL_ACTOR_ID};
use fvm::state_tree::StateTree;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use fvm_shared::address::Address;
use tendermint::account;
use tendermint::PublicKey;
use tendermint::{block::Height, consensus::state};
use tendermint_rpc::Client;

use crate::ExtendVoteInterpreter;

use super::{
    checkpoint::bft_power_table, state::FvmQueryState, store::ReadOnlyBlockstore,
    FvmMessageInterpreter, ValidatorContext,
};

#[async_trait]
impl<DB, TC> ExtendVoteInterpreter for FvmMessageInterpreter<DB, TC>
where
    DB: Blockstore + Clone + 'static + Send + Sync,
    TC: Client + Clone + Send + Sync + 'static,
{
    type State = FvmQueryState<DB>;
    type Message = Tag;

    type Output = Option<bls_signatures::Signature>;

    /// Sign the vote.
    async fn extend_vote(
        &self,
        state: Self::State,
        msg: Self::Message,
    ) -> anyhow::Result<Self::Output> {
        let (state, res) = state.actor_state(&CETFSYSCALL_ACTOR_ADDR).await?;
        let is_enabled = if let Some((_id, act_st)) = res {
            let st: fendermint_actor_cetf::State = state.store_get_cbor(&act_st.state)?.unwrap();
            st.enabled
        } else {
            return Err(anyhow!("no CETF actor found!"));
        };

        if !is_enabled {
            return Ok(None);
        }

        if let Some(ctx) = self.validator_ctx.as_ref() {
            Ok(Some(ctx.sign_tag(&msg)))
        } else {
            Ok(None)
        }
    }

    async fn verify_vote_extension(
        &self,
        state: Self::State,
        msg: Self::Message,
    ) -> anyhow::Result<(Self::State, Self::Output)> {
        if let Some(ctx) = self.validator_ctx.as_ref() {
            let (state, res) = state.actor_state(&CETFSYSCALL_ACTOR_ADDR).await?;
            let store = state.read_only_store();

            let (is_enabled, registered_keys) = if let Some((id, act_st)) = res {
                let st: fendermint_actor_cetf::State =
                    state.store_get_cbor(&act_st.state)?.unwrap();
                (st.enabled, st.get_validators_keymap(&store)?)
            } else {
                return Err(anyhow!("no CETF actor found!"));
            };

            Ok((state, None))
        } else {
            tracing::info!("No validator context found");
            Ok((state, None))
        }
    }
}

pub async fn verify_tag<C, DB>(
    client: &C,
    state: FvmQueryState<DB>,
    validator_pubkey: PublicKey,
    signed_tag: bls_signatures::Signature,
) -> anyhow::Result<bool>
where
    C: Client + Clone + Send + Sync + 'static,
    DB: Blockstore + Send + Sync + Clone + 'static,
{
    let power_table = bft_power_table(client, Height::try_from(state.block_height() as u64)?)
        .await
        .context("failed to get power table")?;

    let bft_keys = power_table
        .0
        .iter()
        .map(|k| k.public_key.clone())
        .collect::<Vec<_>>();

    let (state, res) = state.actor_state(&CETFSYSCALL_ACTOR_ADDR).await?;
    let store = state.read_only_store();

    let (is_enabled, registered_keys) = if let Some((id, act_st)) = res {
        let st: fendermint_actor_cetf::State = state.store_get_cbor(&act_st.state)?.unwrap();
        (st.enabled, st.get_validators_keymap(&store)?)
    } else {
        return Err(anyhow!("no CETF actor found!"));
    };

    if !is_enabled {
        return Ok(false);
    } else {
        // see if every bft key is in the validators map
        for key in bft_keys {
            if !registered_keys
                .contains_key(&Address::new_secp256k1(&key.public_key().serialize())?)?
            {
                return Ok(false);
            }
        }
        // How do we take Tendermint Id to PublicKey?
        // let pubkey = registered_keys.get(key)
    }

    Ok(false)
}
