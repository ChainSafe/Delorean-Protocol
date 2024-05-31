// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{BlockHeight, Tag};
use crate::{BlsPublicKey, BlsSignature};
use cid::Cid;
use fil_actors_runtime::actor_error;
use fil_actors_runtime::{runtime::Runtime, ActorError, Map2};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::tuple::*;
use fvm_sdk::crypto::hash_into;
use fvm_shared::address::Address;
use fvm_shared::crypto::hash::SupportedHashes;

pub type TagMap<BS> = Map2<BS, BlockHeight, Tag>;
pub type ValidatorBlsPublicKeyMap<BS> = Map2<BS, Address, BlsPublicKey>;

pub type SignedHashedTagMap<BS> = Map2<BS, Tag, BlsSignature>;

pub use fil_actors_runtime::DEFAULT_HAMT_CONFIG;
#[derive(Serialize_tuple, Deserialize_tuple, Debug, Clone)]
pub struct State {
    pub tag_map: Cid,    // HAMT[BlockHeight] => Tag
    pub validators: Cid, // HAMT[Address] => BlsPublicKey (Assumes static validator set)
    pub enabled: bool,

    pub signed_hashed_tags: Cid, // HAMT[HashedTag] => BlsSignature(bytes 96)
}

impl State {
    pub fn new<BS: Blockstore>(store: &BS) -> Result<State, ActorError> {
        let tag_map = TagMap::empty(store, DEFAULT_HAMT_CONFIG, "empty tag_map").flush()?;
        let validators =
            ValidatorBlsPublicKeyMap::empty(store, DEFAULT_HAMT_CONFIG, "empty validators")
                .flush()?;

        let signed_hashed_tags =
            SignedHashedTagMap::empty(store, DEFAULT_HAMT_CONFIG, "empty signed_hashed_tags")
                .flush()?;
        Ok(State {
            tag_map,
            validators,
            enabled: false,
            signed_hashed_tags,
        })
    }

    pub fn add_validator<BS: Blockstore>(
        &mut self,
        store: &BS,
        address: &Address,
        public_key: &BlsPublicKey,
    ) -> Result<(), ActorError> {
        let mut validators = ValidatorBlsPublicKeyMap::load(
            store,
            &self.validators,
            DEFAULT_HAMT_CONFIG,
            "reading validators",
        )?;
        validators.set(address, public_key.clone())?;
        self.validators = validators.flush()?;

        Ok(())
    }

    pub fn add_tag_at_height(
        &mut self,
        rt: &impl Runtime,
        height: &BlockHeight,
        tag: &Tag,
    ) -> Result<(), ActorError> {
        let mut tag_map = TagMap::load(
            rt.store(),
            &self.tag_map,
            DEFAULT_HAMT_CONFIG,
            "writing tag_map",
        )?;
        tag_map.set(&height, tag.clone())?;
        self.tag_map = tag_map.flush()?;
        log::info!(
            "Scheduled Cetf Tag for height {}. Current FVM epoch: {}. Tag: {:?}",
            height,
            rt.curr_epoch(),
            tag.0,
        );
        Ok(())
    }

    pub fn get_tag_at_height<BS: Blockstore>(
        &self,
        store: &BS,
        height: &BlockHeight,
    ) -> Result<Option<Tag>, ActorError> {
        let tag_map = TagMap::load(store, &self.tag_map, DEFAULT_HAMT_CONFIG, "reading tag_map")?;
        Ok(tag_map.get(&height)?.copied())
    }

    pub fn get_validators_keymap<BS: Blockstore>(
        &self,
        store: BS,
    ) -> Result<ValidatorBlsPublicKeyMap<BS>, ActorError> {
        ValidatorBlsPublicKeyMap::load(
            store,
            &self.validators,
            DEFAULT_HAMT_CONFIG,
            "reading validators",
        )
    }

    pub fn add_signed_tag_at_height(
        &mut self,
        rt: &impl Runtime,
        height: &BlockHeight,
        signature: &BlsSignature,
    ) -> Result<(), ActorError> {
        let tag = self
            .get_tag_at_height(rt.store(), height)?
            .ok_or_else(|| actor_error!(illegal_state, "Tag not found at height {}", height))?;

        self.add_signed_and_hashed_tag(rt, tag, signature)?;
        log::info!(
            "Added Signed Cetf Tag into map at height {}. FVM epoch: {}.",
            height,
            rt.curr_epoch(),
        );
        log::trace!(
            r#"Tag: {:?}
            Signature: {:?}"#,
            tag.0,
            signature,
        );
        Ok(())
    }

    pub fn add_signed_blockheight_tag_at_height(
        &mut self,
        rt: &impl Runtime,
        height: &BlockHeight,
        signature: &BlsSignature,
    ) -> Result<(), ActorError> {
        let pre = height.to_be_bytes().to_vec();
        let mut digest = [0u8; 32];
        hash_into(SupportedHashes::Sha2_256, &pre, &mut digest);
        self.add_signed_and_hashed_tag(rt, digest.into(), signature)?;
        log::info!(
            "Added Signed BlockHeight into map at reported height {}. FVM epoch: {}.",
            height,
            rt.curr_epoch(),
        );
        log::trace!(
            r#"Height: {:?}
            Hashed Blockheight (tag): {:?}
            Signature: {:?}"#,
            pre,
            digest,
            signature,
        );
        Ok(())
    }

    pub fn add_signed_and_hashed_tag(
        &mut self,
        rt: &impl Runtime,
        tag: Tag,
        signature: &BlsSignature,
    ) -> Result<(), ActorError> {
        let mut signed_hashed_tags = SignedHashedTagMap::load(
            rt.store(),
            &self.signed_hashed_tags,
            DEFAULT_HAMT_CONFIG,
            "writing signed_hashed_tags",
        )?;
        signed_hashed_tags.set(&tag, signature.clone())?;
        self.signed_hashed_tags = signed_hashed_tags.flush()?;
        Ok(())
    }
}
