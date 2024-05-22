// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_ipld_blockstore::Blockstore;
use libipld::Cid;
use libipld::{prelude::*, store::StoreParams, Ipld};

/// Recursively find all [`Cid`] fields in the [`Block`] structures stored in the
/// [`Blockstore`] and return all CIDs which could *not* be retrieved from the store.
///
/// This function is available as a convenience, to be used by any [`BitswapStore`]
/// implementation as they see fit.
pub fn missing_blocks<BS: Blockstore, P: StoreParams>(
    bs: &mut BS,
    cid: &Cid,
) -> anyhow::Result<Vec<Cid>>
where
    Ipld: References<<P as StoreParams>::Codecs>,
{
    let mut stack = vec![*cid];
    let mut missing = vec![];
    while let Some(cid) = stack.pop() {
        if let Some(data) = bs.get(&cid)? {
            let block = libipld::Block::<P>::new_unchecked(cid, data);
            block.references(&mut stack)?;
        } else {
            missing.push(cid);
        }
    }
    Ok(missing)
}
