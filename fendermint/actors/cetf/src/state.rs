// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{BlockHash, BlockHeight, Tag};
use crate::{BlsPublicKey, BlsSignature};
use cid::Cid;
use fil_actors_runtime::{runtime::Runtime, ActorError, Map2, DEFAULT_HAMT_CONFIG};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::tuple::*;
use fvm_shared::address::Address;

pub type TagMap<BS> = Map2<BS, BlockHeight, Tag>;
pub type ValidatorBlsPublicKeyMap<BS> = Map2<BS, Address, BlsPublicKey>;
pub type SignedTagMap<BS> = Map2<BS, BlockHeight, BlsSignature>;

pub type SignedBlockHashTags<BS> = Map2<BS, BlockHash, BlsSignature>;

pub type SignedBlockHeightTags<BS> = Map2<BS, BlockHeight, BlsSignature>;

#[derive(Serialize_tuple, Deserialize_tuple, Debug, Clone)]
pub struct State {
    pub tag_map: Cid,    // HAMT[BlockHeight] => Tag
    pub validators: Cid, // HAMT[Address] => BlsPublicKey (Assumes static validator set)
    pub enabled: bool,

    pub signed_tags: Cid, // HAMT[BlockHeight] => BlsSignature(bytes 96)
    pub signed_blockhash_tags: Cid, // HAMT[Bytes 32] => BlsSignature(bytes 96)
    pub signed_blockheight_tags: Cid, // HAMT[BlockHeight] => BlsSignature(bytes 96)
}

impl State {
    pub fn new<BS: Blockstore>(store: &BS) -> Result<State, ActorError> {
        let tag_map = TagMap::empty(store, DEFAULT_HAMT_CONFIG, "empty tag_map").flush()?;
        let validators =
            ValidatorBlsPublicKeyMap::empty(store, DEFAULT_HAMT_CONFIG, "empty validators")
                .flush()?;

        let signed_tags =
            SignedTagMap::empty(store, DEFAULT_HAMT_CONFIG, "empty signed_tags").flush()?;
        let signed_blockhash_tags =
            SignedBlockHashTags::empty(store, DEFAULT_HAMT_CONFIG, "empty signed_blockhash_tags")
                .flush()?;
        let signed_blockheight_tags = SignedBlockHeightTags::empty(
            store,
            DEFAULT_HAMT_CONFIG,
            "empty signed_blockheight_tags",
        )
        .flush()?;
        Ok(State {
            tag_map,
            validators,
            enabled: false,
            signed_tags,
            signed_blockhash_tags,
            signed_blockheight_tags,
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
        log::info!("Message height: {}", rt.curr_epoch());
        log::info!("Adding signed tag at height {}", height);
        let mut signed_tags = SignedTagMap::load(
            rt.store(),
            &self.signed_tags,
            DEFAULT_HAMT_CONFIG,
            "writing tag_map",
        )?;
        signed_tags.set(&height, signature.clone())?;
        self.signed_tags = signed_tags.flush()?;
        Ok(())
    }

    pub fn add_signed_blockheight_tag_at_height(
        &mut self,
        rt: &impl Runtime,
        height: &BlockHeight,
        signature: &BlsSignature,
    ) -> Result<(), ActorError> {
        log::info!("Message height: {}", rt.curr_epoch());
        log::info!("Adding signed tag at height {}", height);
        let mut signed_blockheight_tags = SignedBlockHeightTags::load(
            rt.store(),
            &self.signed_blockheight_tags,
            DEFAULT_HAMT_CONFIG,
            "writing signed_blockheight_tags",
        )?;
        signed_blockheight_tags.set(&height, signature.clone())?;
        self.signed_blockheight_tags = signed_blockheight_tags.flush()?;
        Ok(())
    }

    pub fn add_signed_blockhash_tag_at_height(
        &mut self,
        rt: &impl Runtime,
        hash: &BlockHash,
        signature: &BlsSignature,
    ) -> Result<(), ActorError> {
        let mut signed_blockhash_tags = SignedBlockHashTags::load(
            rt.store(),
            &self.signed_blockhash_tags,
            DEFAULT_HAMT_CONFIG,
            "writing signed_blockheight_tags",
        )?;
        signed_blockhash_tags.set(&hash, signature.clone())?;
        self.signed_blockhash_tags = signed_blockhash_tags.flush()?;
        Ok(())
    }
}
