// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! Deserialization utils for config mod.

use crate::config::Subnet;
use fvm_shared::address::Address;
use ipc_api::subnet_id::SubnetID;
use ipc_types::EthAddress;
use serde::de::Error;
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::fmt::Formatter;
use std::str::FromStr;

/// A serde deserialization method to deserialize a hashmap of subnets with subnet id as key and
/// Subnet struct as value from a vec of subnets
pub(crate) fn deserialize_subnets_from_vec<'de, D>(
    deserializer: D,
) -> anyhow::Result<HashMap<SubnetID, Subnet>, D::Error>
where
    D: Deserializer<'de>,
{
    let subnets = <Vec<Subnet>>::deserialize(deserializer)?;

    let mut hashmap = HashMap::new();
    for subnet in subnets {
        hashmap.insert(subnet.id.clone(), subnet);
    }
    Ok(hashmap)
}

/// A serde deserialization method to deserialize an address from i64
pub(crate) fn deserialize_address_from_str<'de, D>(
    deserializer: D,
) -> anyhow::Result<Address, D::Error>
where
    D: Deserializer<'de>,
{
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Address;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("an string")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: Error,
        {
            Address::from_str(v).map_err(E::custom)
        }
    }
    deserializer.deserialize_str(Visitor)
}

/// A serde deserialization method to deserialize an eth address from string, i.e. "0x...."
pub fn deserialize_eth_address_from_str<'de, D>(
    deserializer: D,
) -> anyhow::Result<Address, D::Error>
where
    D: Deserializer<'de>,
{
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Address;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("a string")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: Error,
        {
            eth_addr_str_to_address(v).map_err(E::custom)
        }
    }
    deserializer.deserialize_str(Visitor)
}

/// A serde deserialization method to deserialize a subnet path string into a [`SubnetID`].
pub(crate) fn deserialize_subnet_id<'de, D>(deserializer: D) -> anyhow::Result<SubnetID, D::Error>
where
    D: Deserializer<'de>,
{
    struct SubnetIDVisitor;
    impl<'de> serde::de::Visitor<'de> for SubnetIDVisitor {
        type Value = SubnetID;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("a string")
        }

        fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            SubnetID::from_str(v).map_err(E::custom)
        }
    }
    deserializer.deserialize_str(SubnetIDVisitor)
}

fn eth_addr_str_to_address(s: &str) -> anyhow::Result<Address> {
    let addr = EthAddress::from_str(s)?;
    Ok(Address::from(addr))
}
