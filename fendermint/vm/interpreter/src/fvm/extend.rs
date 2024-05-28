// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::HashMap;

use anyhow::anyhow;
use anyhow::Context;
use async_trait::async_trait;
use bls_signatures::Serialize;
use bls_signatures::Serialize as _;
use cid::Cid;
use ethers::types::spoof::State;
use fendermint_actor_cetf::Tag;

use crate::ExtendVoteInterpreter;
use fendermint_vm_actor_interface::cetf::{CETFSYSCALL_ACTOR_ADDR, CETFSYSCALL_ACTOR_ID};
use fvm::state_tree::StateTree;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use fvm_shared::address::Address;
use tendermint::account;
use tendermint::PublicKey;
use tendermint::{block::Height, consensus::state, Hash};
use tendermint_rpc::Client;

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
    type ExtendMessage = Tag;
    type VerifyMessage = (account::Id, Vec<u8>, Option<bls_signatures::Signature>);

    type ExtendOutput = Option<bls_signatures::Signature>;
    type VerifyOutput = Option<bool>;

    /// Sign the vote.
    async fn extend_vote(
        &self,
        state: Self::State,
        msg: Self::ExtendMessage,
    ) -> anyhow::Result<Self::ExtendOutput> {
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
        msg: Self::VerifyMessage,
    ) -> anyhow::Result<(Self::State, Self::VerifyOutput)> {
        let (id, msg, sig) = msg;

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
            if !is_enabled {
                return Ok((state, None));
            }
            // TODO: There must be a better way to convert address to secp256k1 public key

            let key_map =
                bft_power_table(&self.client, Height::try_from(state.block_height() as u64)?)
                    .await
                    .context("failed to get power table")?
                    .0
                    .iter()
                    .map(|k| {
                        let tm_pk: tendermint::PublicKey = k
                            .public_key
                            .clone()
                            .try_into()
                            .expect("failed to convert to secp256k1 public key");

                        let tm_addr = account::Id::from(tm_pk);
                        let fvm_addr =
                            Address::new_secp256k1(&k.public_key.public_key().serialize())
                                .expect("failed to convert to address");
                        (tm_addr, fvm_addr)
                    })
                    .collect::<HashMap<_, _>>();
            let fvm_addr = key_map.get(&id).expect("failed to get fvm address");
            let bls_pub_key = registered_keys
                .get(fvm_addr)
                .expect("failed to get bls public key")
                .unwrap();
            let bls_pub_key = bls_signatures::PublicKey::from_bytes(&bls_pub_key.0)
                .context("failed to deser bls pubkey")?;
            let sig = sig.unwrap();
            let res = bls_signatures::verify_messages(&sig, &[&msg], &[bls_pub_key]);
            tracing::info!("Bls Signature Verification Result: {:?}", res);
            if res {
                Ok((state, Some(true)))
            } else {
                Ok((state, Some(false)))
            }
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
