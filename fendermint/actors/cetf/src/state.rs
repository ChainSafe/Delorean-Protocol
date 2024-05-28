
use fvm_ipld_encoding::tuple::*;
use fil_actors_runtime::{runtime::Runtime, ActorError, Map2, DEFAULT_HAMT_CONFIG};
use fvm_ipld_blockstore::Blockstore;
use cid::Cid;
use crate::{BlockHeight, Tag};

pub type TagMap<BS> = Map2<BS, BlockHeight, Tag>;

#[derive(Serialize_tuple, Deserialize_tuple, Debug, Clone)]
pub struct State {
    pub tag_map: Cid, // HAMT[BlockHeight] => Tag
}

impl State {
    pub fn new<BS: Blockstore>(
        store: &BS,
    ) -> Result<State, ActorError> {
        let mut tag_map = TagMap::empty(store, DEFAULT_HAMT_CONFIG, "empty tag_map");
        let tag_map = tag_map.flush()?;
        Ok(State { tag_map })
    }

    pub fn add_tag_at_height(&self, rt: &impl Runtime, height: &BlockHeight, tag: Tag) -> Result<(), ActorError> {
        let mut tag_map = TagMap::load(rt.store(), &self.tag_map, DEFAULT_HAMT_CONFIG, "writing tag_map")?;
        tag_map.set(height, tag)?;
        Ok(())
    }

    pub fn get_tag_at_height<BS: Blockstore>(&self, store: &BS, height: &BlockHeight) -> Result<Option<Tag>, ActorError> {
        let tag_map = TagMap::load(store, &self.tag_map, DEFAULT_HAMT_CONFIG, "reading tag_map")?;
        Ok(tag_map.get(height)?.copied())
    }
}
