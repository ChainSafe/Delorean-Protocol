use cid::Cid;
// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::lotus::message::CIDMap;

/// A simplified struct representing a `Block` response that does not decode the responses fully.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Block {
    parent_state_root: CIDMap,
}

/// A simplified struct representing a `ChainGetTipSetByHeight` response that does not fully
/// decode the `blocks` field.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetTipSetByHeightResponse {
    pub cids: Vec<CIDMap>,
    blocks: Vec<Block>,
}

impl GetTipSetByHeightResponse {
    pub fn tip_set_cids(&self) -> anyhow::Result<Vec<Cid>> {
        let r: Result<Vec<_>, _> = self
            .cids
            .iter()
            .map(|cid_map| {
                let cid = Cid::try_from(cid_map)?;
                Ok(cid)
            })
            .collect();
        r
    }

    pub fn blocks_state_roots(&self) -> anyhow::Result<Vec<Cid>> {
        self.blocks
            .iter()
            .map(|b| Cid::try_from(&b.parent_state_root))
            .collect()
    }
}

/// A simplified struct representing a `ChainHead` response that does not decode the `blocks` field.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ChainHeadResponse {
    #[allow(dead_code)]
    pub cids: Vec<CIDMap>,
    #[allow(dead_code)]
    pub blocks: Vec<Value>,
    #[allow(dead_code)]
    pub height: u64,
}
