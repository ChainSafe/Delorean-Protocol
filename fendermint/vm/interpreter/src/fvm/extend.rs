// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::HashMap;

use anyhow::anyhow;
use anyhow::Context;
use async_trait::async_trait;
use bls_signatures::Serialize as _;
use fendermint_actor_cetf::BlsSignature;
use fendermint_actor_cetf::Tag;
use num_traits::ToBytes;

use crate::ExtendVoteInterpreter;
use fendermint_vm_actor_interface::cetf::CETFSYSCALL_ACTOR_ADDR;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::serde::{Deserialize, Serialize};
use fvm_shared::address::Address;
use tendermint::account;
use tendermint::block::Height;
use tendermint_rpc::Client;

use super::{
    checkpoint::bft_power_table, state::FvmQueryState, FvmMessageInterpreter, ValidatorContext,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Tags(pub Vec<TagKind>);

#[derive(Debug, Serialize, Deserialize)]
pub struct SignedTags(pub Vec<SignatureKind>);

#[derive(Debug, Serialize, Deserialize)]
pub enum TagKind {
    // From Cetf Actor
    Cetf(Tag),
    // Height as be bytes
    BlockHeight(u64),
}

impl TagKind {
    pub fn to_vec(&self) -> Vec<u8> {
        match self {
            TagKind::Cetf(tag) => tag.to_be_bytes().to_vec(),
            TagKind::BlockHeight(height) => height.to_be_bytes().to_vec(),
        }
    }
    pub fn sign<C>(&self, ctx: &ValidatorContext<C>) -> anyhow::Result<SignatureKind> {
        match self {
            TagKind::Cetf(tag) => {
                let sig = ctx.sign_tag(&tag.to_be_bytes());
                Ok(SignatureKind::Cetf(BlsSignature(
                    sig.as_bytes().try_into().unwrap(),
                )))
            }
            TagKind::BlockHeight(height) => {
                let sig = ctx.sign_tag(&height.to_be_bytes().to_vec());
                Ok(SignatureKind::BlockHeight(BlsSignature(
                    sig.as_bytes().try_into().unwrap(),
                )))
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum SignatureKind {
    Cetf(BlsSignature),
    BlockHeight(BlsSignature),
}

impl SignatureKind {
    pub fn to_vec(&self) -> Vec<u8> {
        match self {
            SignatureKind::Cetf(sig) => sig.0.to_vec(),
            SignatureKind::BlockHeight(sig) => sig.0.to_vec(),
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        match self {
            SignatureKind::Cetf(sig) => sig.0.as_slice(),
            SignatureKind::BlockHeight(sig) => sig.0.as_slice(),
        }
    }
    pub fn to_bls_signature(&self) -> anyhow::Result<bls_signatures::Signature> {
        match self {
            SignatureKind::Cetf(sig) => bls_signatures::Signature::from_bytes(&sig.0),
            SignatureKind::BlockHeight(sig) => bls_signatures::Signature::from_bytes(&sig.0),
        }
        .context("failed to convert SignatureKind to bls signature")
    }
}

#[async_trait]
impl<DB, TC> ExtendVoteInterpreter for FvmMessageInterpreter<DB, TC>
where
    DB: Blockstore + Clone + 'static + Send + Sync,
    TC: Client + Clone + Send + Sync + 'static,
{
    type State = FvmQueryState<DB>;
    type ExtendMessage = Tags;
    type VerifyMessage = (account::Id, Tags, SignedTags);

    type ExtendOutput = SignedTags;
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
            return Ok(SignedTags(vec![]));
        }

        if let Some(ctx) = self.validator_ctx.as_ref() {
            Ok(SignedTags(
                msg.0
                    .iter()
                    .map(|t| t.sign(ctx))
                    .collect::<anyhow::Result<Vec<_>>>()
                    .unwrap(),
            ))
        } else {
            Ok(SignedTags(vec![]))
        }
    }

    async fn verify_vote_extension(
        &self,
        state: Self::State,
        msg: Self::VerifyMessage,
    ) -> anyhow::Result<(Self::State, Self::VerifyOutput)> {
        let (id, tags, sigs) = msg;

        if let Some(_ctx) = self.validator_ctx.as_ref() {
            let (state, res) = state.actor_state(&CETFSYSCALL_ACTOR_ADDR).await?;
            let store = state.read_only_store();

            let (is_enabled, registered_keys) = if let Some((_id, act_st)) = res {
                let st: fendermint_actor_cetf::State =
                    state.store_get_cbor(&act_st.state)?.unwrap();
                (st.enabled, st.get_validators_keymap(&store)?)
            } else {
                return Err(anyhow!("no CETF actor found!"));
            };

            if !is_enabled {
                if !tags.0.is_empty() || !sigs.0.is_empty() {
                    return Err(anyhow!(
                        "CETF Actor is disabled! There should not be and tags or signatures"
                    ));
                }
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

            // Verify signatures
            let mut res = true;
            for (sig, tag) in sigs.0.iter().zip(tags.0.iter()) {
                let v = bls_signatures::verify_messages(
                    &sig.to_bls_signature()?,
                    &[&tag.to_vec()],
                    &[bls_pub_key],
                );
                tracing::info!("BLS Verify for {:?} is {:?}", tag, v);
                res &= v;
            }
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
