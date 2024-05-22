// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use std::collections::HashMap;
use std::hash::Hasher;

use fvm_shared::bigint::Integer;
use fvm_shared::chainid::ChainID;
use lazy_static::lazy_static;
use regex::Regex;
use thiserror::Error;

lazy_static! {
    /// Well known Filecoin chain IDs.
    ///
    /// See all EVM chain IDs at this repo: https://github.com/ethereum-lists/chains/pull/1567
    /// For now I thought it would be enough to enumerate the Filecoin ones.
    static ref KNOWN_CHAIN_IDS: HashMap<u64, &'static str> = HashMap::from([
      (0,        ""), // Used as a default
      (314,      "filecoin"),
      (3141,     "hyperspace"),
      (31415,    "wallaby"),
      (3141592,  "butterflynet"),
      (314159,   "calibnet"),
      (31415926, "devnet"),
    ]);

    /// Reverse index over the chain IDs.
    static ref KNOWN_CHAIN_NAMES: HashMap<&'static str, u64> = KNOWN_CHAIN_IDS.iter().map(|(k, v)| (*v, *k)).collect();

    /// Regex for capturing a single root subnet ID.
    ///
    /// See https://github.com/consensus-shipyard/ipc-actors/pull/109
    static ref ROOT_RE: Regex = Regex::new(r"^/r(0|[1-9]\d*)$").unwrap();
}

/// Maximum value that MetaMask and other Ethereum JS tools can safely handle.
///
/// See https://github.com/ethereum/EIPs/issues/2294
pub const MAX_CHAIN_ID: u64 = 4503599627370476;

#[derive(Error, Debug)]
pub enum ChainIDError {
    /// The name was hashed to a numeric value of a well-known chain.
    /// The chances of this are low, but if it happens, try picking a different name, if possible.
    #[error("illegal name: {0} ({1})")]
    IllegalName(String, u64),
}

/// Hash the name of the chain and reduce it to a number within the acceptable range.
///
/// If the name is one of the well known ones, return the ID for that name as-is.
pub fn from_str_hashed(name: &str) -> Result<ChainID, ChainIDError> {
    // See if the name matches one of the well known chains.
    if let Some(chain_id) = KNOWN_CHAIN_NAMES.get(name) {
        return Ok(ChainID::from(*chain_id));
    }

    // See if the name is actually a rootnet ID like "/r123"
    if let Some(chain_id) = just_root_id(name) {
        return Ok(ChainID::from(chain_id));
    }

    let mut hasher = fnv::FnvHasher::default();
    hasher.write(name.as_bytes());
    let num_digest = hasher.finish();

    let chain_id = num_digest.mod_floor(&MAX_CHAIN_ID);

    if KNOWN_CHAIN_IDS.contains_key(&chain_id) {
        Err(ChainIDError::IllegalName(name.to_owned(), chain_id))
    } else {
        Ok(ChainID::from(chain_id))
    }
}

/// Anything that has a [`ChainID`].
pub trait HasChainID {
    fn chain_id(&self) -> ChainID;
}

/// Extract the root chain ID _iff_ the name is in the format of "/r<chain-id>".
fn just_root_id(name: &str) -> Option<u64> {
    ROOT_RE.captures_iter(name).next().and_then(|cap| {
        let chain_id = &cap[1];
        chain_id.parse::<u64>().ok()
    })
}

#[cfg(test)]
mod tests {

    use fvm_shared::chainid::ChainID;
    use quickcheck_macros::quickcheck;

    use crate::chainid::{just_root_id, KNOWN_CHAIN_NAMES};

    use super::{from_str_hashed, MAX_CHAIN_ID};

    #[quickcheck]
    fn prop_chain_id_stable(name: String) -> bool {
        if let Ok(id1) = from_str_hashed(&name) {
            let id2 = from_str_hashed(&name).unwrap();
            return id1 == id2;
        }
        true
    }

    #[quickcheck]
    fn prop_chain_id_safe(name: String) -> bool {
        if let Ok(id) = from_str_hashed(&name) {
            let chain_id: u64 = id.into();
            return chain_id <= MAX_CHAIN_ID;
        }
        true
    }

    #[test]
    fn chain_id_ok() -> Result<(), String> {
        for name in ["test", "/root/foo/bar"] {
            if let Err(e) = from_str_hashed(name) {
                return Err(format!("failed: {name} - {e}"));
            }
        }
        Ok(())
    }

    #[test]
    fn chain_id_different() {
        let id1 = from_str_hashed("foo").unwrap();
        let id2 = from_str_hashed("bar").unwrap();
        assert_ne!(id1, id2)
    }

    #[test]
    fn chain_id_of_empty_is_zero() {
        assert_eq!(from_str_hashed("").unwrap(), ChainID::from(0))
    }

    #[test]
    fn chain_id_of_known() {
        for (name, id) in KNOWN_CHAIN_NAMES.iter() {
            assert_eq!(from_str_hashed(name).unwrap(), ChainID::from(*id))
        }
    }

    #[test]
    fn chain_id_examples() {
        for (name, id) in [
            ("/r123/f0456/f0789", 3911219601699869),
            ("/foo/bar", 2313053391103756),
        ] {
            assert_eq!(u64::from(from_str_hashed(name).unwrap()), id);
        }
    }

    #[test]
    fn just_root_id_some() {
        assert_eq!(just_root_id("/r0"), Some(0));
        assert_eq!(just_root_id("/r123"), Some(123));

        for (_, id) in KNOWN_CHAIN_NAMES.iter() {
            assert_eq!(
                from_str_hashed(&format!("/r{id}")).unwrap(),
                ChainID::from(*id)
            )
        }
    }

    #[test]
    fn just_root_id_none() {
        for name in [
            "",
            "/",
            "/r",
            "/r01",
            "/r1234567890123456789012345678901234567890",
            "123",
            "abc",
            "/r123/f456",
        ] {
            assert!(just_root_id(name).is_none());
        }
    }
}
