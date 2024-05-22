// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::upper_case_acronyms)] // this is to disable warning for BLS

use std::{convert::TryFrom, fmt::Display, marker::PhantomData, str::FromStr};

use serde::de::Error;

use fvm_shared::address::{Address, Payload};

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct TAddress<T> {
    addr: Address,
    _phantom: PhantomData<T>,
}

impl<T> TAddress<T> {
    pub fn new(addr: Address) -> Self {
        Self {
            addr,
            _phantom: Default::default(),
        }
    }

    #[allow(dead_code)]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.addr.to_bytes()
    }

    /// The untyped `Address` representation.
    #[allow(dead_code)]
    pub fn addr(&self) -> &Address {
        &self.addr
    }
}

trait RawAddress {
    fn is_compatible(addr: Address) -> bool;
}

/// Define a unit struct for address types that can be used as a generic parameter.
macro_rules! raw_address_types {
    ($($typ:ident),+) => {
        $(
        #[derive(PartialEq, Eq, Hash, Clone, Debug)]
        pub struct $typ;

        impl RawAddress for $typ {
          fn is_compatible(addr: Address) -> bool {
            match addr.payload() {
              Payload::$typ(_) => true,
              _ => false
            }
          }
        }
        )*
    };
}

// Based on `Payload` variants.
raw_address_types! {
  ID,
  Secp256k1,
  Actor,
  BLS
}

impl<T> From<TAddress<T>> for Address {
    fn from(t: TAddress<T>) -> Self {
        t.addr
    }
}

impl<A: RawAddress> TryFrom<Address> for TAddress<A> {
    type Error = fvm_shared::address::Error;

    fn try_from(value: Address) -> Result<Self, Self::Error> {
        if !A::is_compatible(value) {
            return Err(fvm_shared::address::Error::InvalidPayload);
        }
        Ok(Self {
            addr: value,
            _phantom: PhantomData,
        })
    }
}

/// Serializes exactly as its underlying `Address`.
impl<T> serde::Serialize for TAddress<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.addr.serialize(serializer)
    }
}

/// Deserializes exactly as its underlying `Address` but might be rejected if it's not the expected type.
impl<'d, T> serde::Deserialize<'d> for TAddress<T>
where
    Self: TryFrom<Address>,
    <Self as TryFrom<Address>>::Error: Display,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'d>,
    {
        let raw = Address::deserialize(deserializer)?;
        match Self::try_from(raw) {
            Ok(addr) => Ok(addr),
            Err(e) => Err(D::Error::custom(format!("wrong address type: {e}"))),
        }
    }
}

/// Apparently CBOR has problems using `Address` as a key in `HashMap`.
/// This type can be used to wrap an address and turn it into `String`
/// for the purpose of CBOR serialization.
#[derive(PartialEq, Eq, Hash, Clone, Debug)]
pub struct TAddressKey<T>(pub TAddress<T>);

/// Serializes to the `String` format of the underlying `Address`.
impl<T> serde::Serialize for TAddressKey<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.addr.to_string().serialize(serializer)
    }
}

/// Deserializes from `String` format. May be rejected if the address is not the expected type.
impl<'d, T> serde::Deserialize<'d> for TAddressKey<T>
where
    TAddress<T>: TryFrom<Address>,
    <TAddress<T> as TryFrom<Address>>::Error: Display,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'d>,
    {
        let str = String::deserialize(deserializer)?;
        let raw = Address::from_str(&str)
            .map_err(|e| D::Error::custom(format!("not an address string: {e:?}")))?;
        let addr = TAddress::<T>::try_from(raw)
            .map_err(|e| D::Error::custom(format!("wrong address type: {e}")))?;
        Ok(Self(addr))
    }
}
