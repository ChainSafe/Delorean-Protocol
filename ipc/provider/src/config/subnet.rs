use std::time::Duration;

// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use fvm_shared::address::Address;
use ipc_api::subnet_id::SubnetID;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DurationSeconds};
use url::Url;

use crate::config::deserialize::{
    deserialize_address_from_str, deserialize_eth_address_from_str, deserialize_subnet_id,
};
use crate::config::serialize::{
    serialize_address_to_str, serialize_eth_address_to_str, serialize_subnet_id_to_str,
};

/// Represents a subnet declaration in the config.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Subnet {
    #[serde(deserialize_with = "deserialize_subnet_id")]
    #[serde(serialize_with = "serialize_subnet_id_to_str")]
    pub id: SubnetID,
    pub config: SubnetConfig,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "network_type")]
pub enum SubnetConfig {
    #[serde(rename = "fevm")]
    Fevm(EVMSubnet),
}

/// A helper enum to differentiate the different network types
#[derive(PartialEq, Eq)]
pub enum NetworkType {
    Fevm,
}

impl Subnet {
    pub fn network_type(&self) -> NetworkType {
        match &self.config {
            SubnetConfig::Fevm(_) => NetworkType::Fevm,
        }
    }

    pub fn auth_token(&self) -> Option<String> {
        match &self.config {
            SubnetConfig::Fevm(s) => s.auth_token.clone(),
        }
    }

    pub fn rpc_http(&self) -> &Url {
        match &self.config {
            SubnetConfig::Fevm(s) => &s.provider_http,
        }
    }

    pub fn rpc_timeout(&self) -> Option<Duration> {
        match &self.config {
            SubnetConfig::Fevm(s) => s.provider_timeout,
        }
    }

    pub fn gateway_addr(&self) -> Address {
        match &self.config {
            SubnetConfig::Fevm(s) => s.gateway_addr,
        }
    }
}

/// The FVM subnet config parameters
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct FVMSubnet {
    #[serde(deserialize_with = "deserialize_address_from_str")]
    #[serde(serialize_with = "serialize_address_to_str")]
    pub gateway_addr: Address,
    pub jsonrpc_api_http: Url,
    pub auth_token: Option<String>,
}

/// The EVM subnet config parameters
#[serde_as]
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct EVMSubnet {
    pub provider_http: Url,
    #[serde_as(as = "Option<DurationSeconds<u64>>")]
    pub provider_timeout: Option<Duration>,
    pub auth_token: Option<String>,

    #[serde(deserialize_with = "deserialize_eth_address_from_str")]
    #[serde(serialize_with = "serialize_eth_address_to_str")]
    pub registry_addr: Address,

    #[serde(deserialize_with = "deserialize_eth_address_from_str")]
    #[serde(serialize_with = "serialize_eth_address_to_str")]
    pub gateway_addr: Address,
}
