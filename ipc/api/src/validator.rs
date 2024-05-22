// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT

use fvm_shared::{address::Address, econ::TokenAmount};
use ipc_actors_abis::subnet_actor_getter_facet;

use crate::{
    eth_to_fil_amount, ethers_address_to_fil_address,
    evm::{fil_to_eth_amount, payload_to_evm_address},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Validator {
    pub addr: Address,
    pub metadata: Vec<u8>,
    pub weight: TokenAmount,
}

impl TryFrom<Validator> for subnet_actor_getter_facet::Validator {
    type Error = anyhow::Error;

    fn try_from(value: Validator) -> Result<Self, Self::Error> {
        Ok(subnet_actor_getter_facet::Validator {
            addr: payload_to_evm_address(value.addr.payload())?,
            weight: fil_to_eth_amount(&value.weight)?,
            metadata: ethers::core::types::Bytes::from(value.metadata),
        })
    }
}

pub fn into_contract_validators(
    vals: Vec<Validator>,
) -> anyhow::Result<Vec<subnet_actor_getter_facet::Validator>> {
    let result: Result<Vec<subnet_actor_getter_facet::Validator>, _> = vals
        .into_iter()
        .map(|validator| validator.try_into())
        .collect();

    result
}

pub fn from_contract_validators(
    vals: Vec<subnet_actor_getter_facet::Validator>,
) -> anyhow::Result<Vec<Validator>> {
    let result: Result<Vec<Validator>, _> = vals
        .into_iter()
        .map(|validator| {
            Ok(Validator {
                addr: ethers_address_to_fil_address(&validator.addr)?,
                weight: eth_to_fil_amount(&validator.weight)?,
                metadata: validator.metadata.to_vec(),
            })
        })
        .collect();

    result
}
