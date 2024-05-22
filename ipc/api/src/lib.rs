// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use ethers::utils::hex;
use fvm_shared::{address::Address, econ::TokenAmount};
use ipc_types::EthAddress;
use serde::de::Error as SerdeError;
use serde::{Deserialize, Serialize, Serializer};
use std::str::FromStr;

pub mod address;
pub mod checkpoint;
pub mod cross;
pub mod error;
pub mod gateway;
#[cfg(feature = "fil-actor")]
mod runtime;
pub mod subnet;
pub mod subnet_id;
pub mod validator;

pub mod evm;
pub mod staking;

/// Converts an ethers::U256 TokenAmount into a FIL amount.
pub fn eth_to_fil_amount(amount: &ethers::types::U256) -> anyhow::Result<TokenAmount> {
    let v = fvm_shared::bigint::BigInt::from_str(&amount.to_string())?;
    Ok(TokenAmount::from_atto(v))
}

pub fn ethers_address_to_fil_address(addr: &ethers::types::Address) -> anyhow::Result<Address> {
    let raw_addr = format!("{addr:?}");
    log::debug!("raw evm subnet addr: {raw_addr:}");

    let eth_addr = EthAddress::from_str(&raw_addr)?;
    Ok(Address::from(eth_addr))
}

/// Marker type for serialising data to/from string
pub struct HumanReadable;

#[macro_export]
macro_rules! serialise_human_readable_str {
    ($typ:ty) => {
        impl serde_with::SerializeAs<$typ> for $crate::HumanReadable {
            fn serialize_as<S>(value: &$typ, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                if serializer.is_human_readable() {
                    value.to_string().serialize(serializer)
                } else {
                    value.serialize(serializer)
                }
            }
        }
    };
}

#[macro_export]
macro_rules! deserialize_human_readable_str {
    ($typ:ty) => {
        use serde::de::Error as DeserializeError;
        use serde::{Deserialize, Serialize};

        impl<'de> serde_with::DeserializeAs<'de, $typ> for $crate::HumanReadable {
            fn deserialize_as<D>(deserializer: D) -> Result<$typ, D::Error>
            where
                D: serde::de::Deserializer<'de>,
            {
                if deserializer.is_human_readable() {
                    let s = String::deserialize(deserializer)?;
                    <$typ>::from_str(&s).map_err(|_| {
                        D::Error::custom(format!(
                            "cannot parse from str {}",
                            core::any::type_name::<$typ>()
                        ))
                    })
                } else {
                    <$typ>::deserialize(deserializer)
                }
            }
        }
    };
}

#[macro_export]
macro_rules! as_human_readable_str {
    ($typ:ty) => {
        $crate::serialise_human_readable_str!($typ);
        $crate::deserialize_human_readable_str!($typ);
    };
}

impl serde_with::SerializeAs<Vec<u8>> for HumanReadable {
    fn serialize_as<S>(source: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        hex::encode(source).serialize(serializer)
    }
}

impl<'de> serde_with::DeserializeAs<'de, Vec<u8>> for HumanReadable {
    fn deserialize_as<D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            Ok(hex::decode(s)
                .map_err(|e| D::Error::custom(format!("cannot parse from str {e}")))?)
        } else {
            Vec::<u8>::deserialize(deserializer)
        }
    }
}
