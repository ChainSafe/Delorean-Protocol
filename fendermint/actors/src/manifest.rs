// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use anyhow::{anyhow, Context};
use cid::Cid;
use fendermint_actor_chainmetadata::CHAINMETADATA_ACTOR_NAME;
use fendermint_actor_eam::IPC_EAM_ACTOR_NAME;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use std::collections::HashMap;

// array of required actors
pub const REQUIRED_ACTORS: &[&str] = &[CHAINMETADATA_ACTOR_NAME, IPC_EAM_ACTOR_NAME];

/// A mapping of internal actor CIDs to their respective types.
pub struct Manifest {
    code_by_name: HashMap<String, Cid>,
}

impl Manifest {
    /// Load a manifest from the blockstore.
    pub fn load<B: Blockstore>(bs: &B, root_cid: &Cid, ver: u32) -> anyhow::Result<Manifest> {
        if ver != 1 {
            return Err(anyhow!("unsupported manifest version {}", ver));
        }

        let vec: Vec<(String, Cid)> = match bs.get_cbor(root_cid)? {
            Some(vec) => vec,
            None => {
                return Err(anyhow!("cannot find manifest root cid {}", root_cid));
            }
        };

        Manifest::new(vec)
    }

    /// Construct a new manifest from actor name/cid tuples.
    pub fn new(iter: impl IntoIterator<Item = (impl Into<String>, Cid)>) -> anyhow::Result<Self> {
        let mut code_by_name = HashMap::new();
        for (name, code_cid) in iter.into_iter() {
            code_by_name.insert(name.into(), code_cid);
        }

        // loop over required actors and ensure they are present
        for &name in REQUIRED_ACTORS.iter() {
            let _ = code_by_name
                .get(name)
                .with_context(|| format!("manifest missing required actor {}", name))?;
        }

        Ok(Self { code_by_name })
    }

    pub fn code_by_name(&self, str: &str) -> Option<&Cid> {
        self.code_by_name.get(str)
    }
}
