// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
//! Handles the type conversion to ethers contract types

use crate::IPCParentFinality;
use anyhow::anyhow;
use ethers::types::U256;
use ipc_actors_abis::{gateway_getter_facet, top_down_finality_facet};

impl TryFrom<IPCParentFinality> for top_down_finality_facet::ParentFinality {
    type Error = anyhow::Error;

    fn try_from(value: IPCParentFinality) -> Result<Self, Self::Error> {
        if value.block_hash.len() != 32 {
            return Err(anyhow!("invalid block hash length, expecting 32"));
        }

        let mut block_hash = [0u8; 32];
        block_hash.copy_from_slice(&value.block_hash[0..32]);

        Ok(Self {
            height: U256::from(value.height),
            block_hash,
        })
    }
}

impl From<gateway_getter_facet::ParentFinality> for IPCParentFinality {
    fn from(value: gateway_getter_facet::ParentFinality) -> Self {
        IPCParentFinality {
            height: value.height.as_u64(),
            block_hash: value.block_hash.to_vec(),
        }
    }
}

impl From<top_down_finality_facet::ParentFinality> for IPCParentFinality {
    fn from(value: top_down_finality_facet::ParentFinality) -> Self {
        IPCParentFinality {
            height: value.height.as_u64(),
            block_hash: value.block_hash.to_vec(),
        }
    }
}
