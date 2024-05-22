// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
//! Helper data structures to declare diamond pattern contracts.

// See https://medium.com/@MarqyMarq/how-to-implement-the-diamond-standard-69e87dae44e6

use std::collections::HashMap;

use ethers::abi::Abi;
use fvm_shared::ActorID;

#[derive(Clone, Debug)]
pub struct EthFacet {
    pub name: &'static str,
    pub abi: Abi,
}

/// Top level Ethereum contract with a pre-determined ID.
#[derive(Clone, Debug)]
pub struct EthContract {
    /// Pre-determined ID for the contract.
    ///
    /// 0 means the contract will get a dynamic ID.
    pub actor_id: ActorID,
    pub abi: Abi,
    /// List of facets if the contract is using the diamond pattern.
    pub facets: Vec<EthFacet>,
}

pub type EthContractMap = HashMap<&'static str, EthContract>;
