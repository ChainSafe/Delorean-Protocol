// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt::{Debug, Display};

use cid::multihash::MultihashDigest;
use fendermint_crypto::PublicKey;
use fvm_ipld_encoding::{
    strict_bytes,
    tuple::{Deserialize_tuple, Serialize_tuple},
};
use fvm_shared::{
    address::{Address, Error, SECP_PUB_LEN},
    ActorID, METHOD_CONSTRUCTOR,
};

define_singleton!(EAM {
    id: 10,
    code_id: 15
});

pub const EAM_ACTOR_NAME: &str = "eam";

/// Ethereum Address Manager actor methods available.
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    Create = 2,
    Create2 = 3,
    CreateExternal = 4,
}

// TODO: We could re-export `fil_evm_actor_shared::address::EvmAddress`.
#[derive(
    serde::Deserialize, serde::Serialize, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default,
)]
pub struct EthAddress(#[serde(with = "strict_bytes")] pub [u8; 20]);

impl EthAddress {
    /// Returns an EVM-form ID address from actor ID.
    ///
    /// This is copied from the `evm` actor library.
    pub fn from_id(id: u64) -> Self {
        let mut bytes = [0u8; 20];
        bytes[0] = 0xff;
        bytes[12..].copy_from_slice(&id.to_be_bytes());
        Self(bytes)
    }

    /// Hash the public key according to the Ethereum convention.
    pub fn new_secp256k1(pubkey: &[u8]) -> Result<Self, Error> {
        if pubkey.len() != SECP_PUB_LEN {
            return Err(Error::InvalidSECPLength(pubkey.len()));
        }
        let mut hash20 = [0u8; 20];
        // Based on [ethers_core::utils::secret_key_to_address]
        let hash32 = cid::multihash::Code::Keccak256.digest(&pubkey[1..]);
        hash20.copy_from_slice(&hash32.digest()[12..]);
        Ok(Self(hash20))
    }

    /// Indicate whether this hash is really an actor ID.
    pub fn is_masked_id(&self) -> bool {
        self.0[0] == 0xff && self.0[1..].starts_with(&[0u8; 11])
    }
}

impl Display for EthAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&ethers::types::Address::from(self.0), f)
    }
}

impl Debug for EthAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(&ethers::types::Address::from(self.0), f)
    }
}

impl From<EthAddress> for Address {
    fn from(value: EthAddress) -> Address {
        if value.is_masked_id() {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&value.0[12..]);
            let id = u64::from_be_bytes(bytes);
            Address::new_id(id)
        } else {
            Address::new_delegated(EAM_ACTOR_ID, &value.0).expect("EthAddress is delegated")
        }
    }
}

impl From<EthAddress> for ethers::types::Address {
    fn from(value: EthAddress) -> Self {
        Self(value.0)
    }
}

impl From<&EthAddress> for ethers::types::Address {
    fn from(value: &EthAddress) -> Self {
        Self(value.0)
    }
}

impl From<ethers::types::Address> for EthAddress {
    fn from(value: ethers::types::Address) -> Self {
        Self(value.0)
    }
}

impl From<PublicKey> for EthAddress {
    fn from(value: PublicKey) -> Self {
        Self::new_secp256k1(&value.serialize()).expect("length is 65")
    }
}

impl AsRef<[u8]> for EthAddress {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Helper to read return value from contract creation.
#[derive(Serialize_tuple, Deserialize_tuple, Debug, Clone)]
pub struct CreateReturn {
    pub actor_id: ActorID,
    pub robust_address: Option<Address>,
    pub eth_address: EthAddress,
}

impl CreateReturn {
    /// Delegated EAM address of the EVM actor, which can be used to invoke the contract.
    pub fn delegated_address(&self) -> Address {
        Address::new_delegated(EAM_ACTOR_ID, &self.eth_address.0).expect("ETH address should work")
    }
}

#[cfg(test)]
mod tests {
    use ethers_core::k256::ecdsa::SigningKey;
    use fendermint_crypto::SecretKey;
    use quickcheck_macros::quickcheck;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    use super::EthAddress;

    #[quickcheck]
    fn prop_new_secp256k1(seed: u64) -> bool {
        let mut rng = StdRng::seed_from_u64(seed);
        let sk = SecretKey::random(&mut rng);

        let signing_key = SigningKey::from_slice(sk.serialize().as_ref()).unwrap();
        let address = ethers_core::utils::secret_key_to_address(&signing_key);

        let eth_address = EthAddress::from(sk.public_key());

        address.0 == eth_address.0
    }
}
