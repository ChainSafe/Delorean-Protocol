// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::METHOD_CONSTRUCTOR;
use serde_tuple::{Deserialize_tuple, Serialize_tuple};

pub use fil_actors_evm_shared::uints;

use crate::eam::EthAddress;

define_code!(EVM { code_id: 14 });

#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    Resurrect = 2,
    GetBytecode = 3,
    GetBytecodeHash = 4,
    GetStorageAt = 5,
    InvokeContractDelegate = 6,
    // This hardcoded value is taken from https://github.com/filecoin-project/ref-fvm/blob/f4f3f340ba29b3800cd8272e34023606def23855/testing/integration/src/testkit/fevm.rs#L88-L89
    // where it's used because of a ciruclar dependency (frc42_dispatch needs fvm_shared).
    // Here we can use it if we want, however the release cycle is a bit lagging, preventing us from using the latest ref-fvm at the moment.
    //InvokeContract = frc42_dispatch::method_hash!("InvokeEVM"),
    InvokeContract = 3844450837,
}

// XXX: I don't know why the following arent' part of `fil_actors_evm_shared` :(

#[derive(Serialize_tuple, Deserialize_tuple)]
#[serde(transparent)]
pub struct BytecodeReturn {
    pub code: Option<Cid>,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct GetStorageAtParams {
    pub storage_key: uints::U256,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
#[serde(transparent)]
pub struct GetStorageAtReturn {
    pub storage: uints::U256,
}

#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct ConstructorParams {
    /// The actor's "creator" (specified by the EAM).
    pub creator: EthAddress,
    /// The initcode that will construct the new EVM actor.
    pub initcode: RawBytes,
}

/// Define an error type that implements [ContractRevert] and is a union
/// of multiple other such types. Intended to be used when a contract
/// calls other contracts that can also revert with known custom error
/// types, so that we can get something readable even if the error doesn't
/// directly come from the contract we call.
///
/// # Example
/// ```ignore
/// revert_errors! {
///    SubnetActorErrors {
///        SubnetActorManagerFacetErrors,
///        GatewayManagerFacetErrors
///    }
/// }
/// ```
#[macro_export]
macro_rules! revert_errors {
    ($typ:ident {$($elem:ident),+}) => {
        #[derive(Debug, Clone, Eq, PartialEq, Hash)]
        pub enum $typ {
            $($elem($elem),)+
        }

        impl ::ethers::core::abi::AbiDecode for $typ {
            fn decode(data: impl AsRef<[u8]>) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
                let data = data.as_ref();
                $(
                    if let Ok(decoded) =
                        <$elem as ::ethers::core::abi::AbiDecode>::decode(data)
                    {
                        return Ok(Self::$elem(decoded));
                    }
                )+
                Err(::ethers::core::abi::Error::InvalidData.into())
            }
        }

        impl ::ethers::core::abi::AbiEncode for $typ {
            fn encode(self) -> ::std::vec::Vec<u8> {
                match self {
                $(
                    Self::$elem(element) => ::ethers::core::abi::AbiEncode::encode(element),
                )+
                }
            }
        }

        impl ::ethers::contract::ContractRevert for $typ {
            fn valid_selector(selector: [u8; 4]) -> bool {
                $(
                    if <$elem as ::ethers::contract::ContractRevert>::valid_selector(selector) {
                        return true;
                    }
                )+
                false
            }
        }

        impl ::core::fmt::Display for $typ {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                $(
                    Self::$elem(element) => ::core::fmt::Display::fmt(element, f),
                )+
                }
            }
        }
    };
}
