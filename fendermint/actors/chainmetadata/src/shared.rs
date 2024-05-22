// Copyright 2021-2023 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use fvm_ipld_amt::Amt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::tuple::{Deserialize_tuple, Serialize_tuple};
use fvm_shared::{clock::ChainEpoch, METHOD_CONSTRUCTOR};
use num_derive::FromPrimitive;

// The state stores the blockhashes of the last `lookback_len` epochs
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct State {
    // the AMT root cid of blockhashes
    //
    // TODO: consider using kamt instead due to appending larger and
    // larger keys to the AMT makes it unbalanced requiring more space
    // to store (see https://github.com/filecoin-project/go-amt-ipld/issues/17)
    pub blockhashes: Cid,

    // the maximum size of blockhashes before removing the oldest epoch
    pub lookback_len: u64,
}

impl State {
    pub fn new<BS: Blockstore>(store: &BS, lookback_len: u64) -> anyhow::Result<Self> {
        let empty_blockhashes_cid =
            match Amt::<(), _>::new_with_bit_width(store, BLOCKHASHES_AMT_BITWIDTH).flush() {
                Ok(cid) => cid,
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "chainmetadata actor failed to create empty Amt: {}",
                        e
                    ))
                }
            };

        Ok(Self {
            blockhashes: empty_blockhashes_cid,
            lookback_len,
        })
    }

    // loads the blockhashes array from the AMT root cid and returns the blockhash
    // at the given epoch
    pub fn get_block_hash<BS: Blockstore>(
        &self,
        store: &BS,
        epoch: ChainEpoch,
    ) -> anyhow::Result<Option<BlockHash>> {
        // load the blockhashes array from the AMT root cid
        let blockhashes = match Amt::load(&self.blockhashes, &store) {
            Ok(v) => v,
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "failed to load blockhashes from AMT cid {}, error: {}",
                    self.blockhashes,
                    e
                ));
            }
        };

        // get the block hash at the given epoch
        match blockhashes.get(epoch as u64) {
            Ok(Some(v)) => Ok(Some(*v)),
            Ok(None) => Ok(None),
            Err(err) => Err(anyhow::anyhow!(
                "failed to get blockhash at epoch {}, error: {}",
                epoch,
                err
            )),
        }
    }
}

pub const CHAINMETADATA_ACTOR_NAME: &str = "chainmetadata";

// the default lookback length is 256 epochs
pub const DEFAULT_LOOKBACK_LEN: u64 = 256;

// the default bitwidth of the blockhashes AMT
pub const BLOCKHASHES_AMT_BITWIDTH: u32 = 3;

#[derive(Default, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct ConstructorParams {
    pub lookback_len: u64,
}

pub type BlockHash = [u8; 32];

#[derive(Default, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct PushBlockParams {
    pub epoch: ChainEpoch,
    pub block: BlockHash,
}

#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    PushBlockHash = frc42_dispatch::method_hash!("PushBlockHash"),
    LookbackLen = frc42_dispatch::method_hash!("LookbackLen"),
    GetBlockHash = frc42_dispatch::method_hash!("GetBlockHash"),
}
