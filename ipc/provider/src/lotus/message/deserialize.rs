// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! Deserialization utils for lotus/ipc types.

use fvm_shared::address::Address;
use fvm_shared::bigint::BigInt;
use fvm_shared::econ::TokenAmount;
use ipc_api::address::IPCAddress;
use ipc_api::subnet_id::SubnetID;
use serde::de::{Error, MapAccess};
use serde::{Deserialize, Deserializer};
use std::fmt::Formatter;
use std::str::FromStr;

/// A serde deserialization method to deserialize a ipc address from map
pub fn deserialize_ipc_address_from_map<'de, D>(
    deserializer: D,
) -> anyhow::Result<IPCAddress, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct IPCAddressInner {
        #[serde(deserialize_with = "deserialize_subnet_id_from_map")]
        pub subnet_id: SubnetID,
        #[serde(deserialize_with = "deserialize_address_from_str")]
        pub raw_address: Address,
    }

    let inner = IPCAddressInner::deserialize(deserializer)?;
    let ipc = IPCAddress::new(&inner.subnet_id, &inner.raw_address).map_err(D::Error::custom)?;
    Ok(ipc)
}

/// A serde deserialization method to deserialize a subnet id from map
pub fn deserialize_subnet_id_from_map<'de, D>(deserializer: D) -> anyhow::Result<SubnetID, D::Error>
where
    D: Deserializer<'de>,
{
    struct SubnetIdVisitor;
    impl<'de> serde::de::Visitor<'de> for SubnetIdVisitor {
        type Value = SubnetID;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("a map")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let mut root = None;
            let mut children = None;
            while let Some((key, value)) = map
                .next_entry()?
                .map(|(k, v): (String, &'de serde_json::value::RawValue)| (k, v))
            {
                match key.as_str() {
                    "Root" => {
                        let s = value.get();
                        // ideally root should be an integer, if it starts with ", it is sent as a
                        // string, we strip away the starting and ending quote.
                        let id: u64 = if s.starts_with('"') {
                            s[1..s.len() - 1].parse().map_err(A::Error::custom)?
                        } else {
                            s.parse().map_err(A::Error::custom)?
                        };
                        root = Some(id);
                    }
                    "Children" => {
                        let s = value.get();
                        let v: Vec<String> = serde_json::from_str(s).map_err(A::Error::custom)?;
                        let addr: Result<Vec<Address>, A::Error> = v
                            .iter()
                            .map(|s| Address::from_str(s).map_err(A::Error::custom))
                            .collect();
                        children = Some(addr?);
                    }
                    _ => {}
                }
            }

            if root.is_none() || children.is_none() {
                return Err(A::Error::custom("parent or actor not present"));
            }

            Ok(SubnetID::new(root.unwrap(), children.unwrap()))
        }
    }
    deserializer.deserialize_map(SubnetIdVisitor)
}

/// A serde deserialization method to deserialize a token amount from string
pub fn deserialize_token_amount_from_str<'de, D>(
    deserializer: D,
) -> anyhow::Result<TokenAmount, D::Error>
where
    D: Deserializer<'de>,
{
    struct TokenAmountVisitor;
    impl<'de> serde::de::Visitor<'de> for TokenAmountVisitor {
        type Value = TokenAmount;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("a string")
        }

        fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            let u = BigInt::from_str(v).map_err(E::custom)?;
            Ok(TokenAmount::from_atto(u))
        }
    }
    deserializer.deserialize_str(TokenAmountVisitor)
}

/// A serde deserialization method to deserialize a token amount from string
pub fn deserialize_some_token_amount_from_str<'de, D>(
    deserializer: D,
) -> anyhow::Result<Option<TokenAmount>, D::Error>
where
    D: Deserializer<'de>,
{
    struct TokenAmountVisitor;
    impl<'de> serde::de::Visitor<'de> for TokenAmountVisitor {
        type Value = Option<TokenAmount>;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("a string")
        }

        fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            let u = BigInt::from_str(v).map_err(E::custom)?;
            Ok(Some(TokenAmount::from_atto(u)))
        }
    }
    deserializer.deserialize_str(TokenAmountVisitor)
}

/// A serde deserialization method to deserialize a token amount from i64
pub fn deserialize_some_token_amount_from_num<'de, D>(
    deserializer: D,
) -> anyhow::Result<Option<TokenAmount>, D::Error>
where
    D: Deserializer<'de>,
{
    let val: serde_json::Value = serde::Deserialize::deserialize(deserializer)?;
    match val {
        serde_json::Value::Number(n) => {
            let u = n
                .as_u64()
                .ok_or_else(|| D::Error::custom("cannot parse to u64"))?;
            Ok(Some(TokenAmount::from_atto(u)))
        }
        _ => Err(D::Error::custom("unknown type: {val:?}")),
    }
}

/// A serde deserialization method to deserialize an address from string
pub fn deserialize_address_from_str<'de, D>(deserializer: D) -> anyhow::Result<Address, D::Error>
where
    D: Deserializer<'de>,
{
    struct AddressVisitor;
    impl<'de> serde::de::Visitor<'de> for AddressVisitor {
        type Value = Address;

        fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
            formatter.write_str("a string")
        }

        fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
        where
            E: Error,
        {
            Address::from_str(v).map_err(E::custom)
        }
    }
    deserializer.deserialize_str(AddressVisitor)
}
