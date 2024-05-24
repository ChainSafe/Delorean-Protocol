// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::BlsPublicKey;
use crate::{BlockHeight, Tag};
use cid::Cid;
use fil_actors_runtime::{runtime::Runtime, ActorError, Map2, DEFAULT_HAMT_CONFIG};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::tuple::*;
use fvm_shared::address::Address;

pub type TagMap<BS> = Map2<BS, BlockHeight, Tag>;
pub type ValidatorBlsPublicKeyMap<BS> = Map2<BS, Address, BlsPublicKey>;

#[derive(Serialize_tuple, Deserialize_tuple, Debug, Clone)]
pub struct State {
    pub tag_map: Cid,    // HAMT[BlockHeight] => Tag
    pub validators: Cid, // HAMT[Address] => BlsPublicKey (Assumes static validator set)
    pub enabled: bool,
}

impl State {
    pub fn new<BS: Blockstore>(store: &BS) -> Result<State, ActorError> {
        let tag_map = { TagMap::empty(store, DEFAULT_HAMT_CONFIG, "empty tag_map").flush()? };
        let validators = { TagMap::empty(store, DEFAULT_HAMT_CONFIG, "empty validators").flush()? };
        Ok(State {
            tag_map,
            validators,
            enabled: false,
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
}
