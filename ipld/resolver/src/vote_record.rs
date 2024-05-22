// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use ipc_api::subnet_id::SubnetID;
use libp2p::identity::{Keypair, PublicKey};
use serde::de::{DeserializeOwned, Error};
use serde::{Deserialize, Serialize};

use crate::{
    signed_record::{Record, SignedRecord},
    Timestamp,
};

/// The basic idea is that validators, identified by their public key,
/// vote about things regarding the subnet in which they participate.
#[derive(Debug, Clone, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct ValidatorKey(PublicKey);

impl Serialize for ValidatorKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let bz = self.0.encode_protobuf();
        bz.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ValidatorKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bz = Vec::<u8>::deserialize(deserializer)?;
        match PublicKey::try_decode_protobuf(&bz) {
            Ok(pk) => Ok(Self(pk)),
            Err(e) => Err(D::Error::custom(format!("error decoding PublicKey: {e}"))),
        }
    }
}

impl From<PublicKey> for ValidatorKey {
    fn from(value: PublicKey) -> Self {
        Self(value)
    }
}

impl From<ValidatorKey> for PublicKey {
    fn from(value: ValidatorKey) -> Self {
        value.0
    }
}

impl From<libsecp256k1::PublicKey> for ValidatorKey {
    fn from(value: libsecp256k1::PublicKey) -> Self {
        let public_key =
            libp2p::identity::secp256k1::PublicKey::try_from_bytes(&value.serialize_compressed())
                .expect("secp256k1 public key");

        Self::from(PublicKey::from(public_key))
    }
}

/// Vote by a validator about the validity/availability/finality
/// of something in a given subnet.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct VoteRecord<C> {
    /// Public key of the validator.
    pub public_key: ValidatorKey,
    /// The subnet in which the vote is valid, to prevent a vote on the same subject
    /// in one subnet being replayed by an attacker on a different subnet.
    pub subnet_id: SubnetID,
    /// The content the vote is about.
    pub content: C,
    /// Timestamp to thwart potential replay attacks.
    pub timestamp: Timestamp,
}

impl<C> Record for VoteRecord<C> {
    fn payload_type() -> &'static str {
        "/ipc/vote-record"
    }

    fn check_signing_key(&self, key: &PublicKey) -> bool {
        self.public_key.0 == *key
    }
}

pub type SignedVoteRecord<C> = SignedRecord<VoteRecord<C>>;

impl<C> VoteRecord<C>
where
    C: Serialize + DeserializeOwned,
{
    /// Create a new [`SignedVoteRecord`] with the current timestamp
    /// and a signed envelope which can be shared with others.
    pub fn signed(
        key: &Keypair,
        subnet_id: SubnetID,
        content: C,
    ) -> anyhow::Result<SignedVoteRecord<C>> {
        let timestamp = Timestamp::now();
        let record = VoteRecord {
            public_key: ValidatorKey(key.public()),
            subnet_id,
            content,
            timestamp,
        };
        let signed = SignedRecord::new(key, record)?;
        Ok(signed)
    }
}

#[cfg(any(test, feature = "arb"))]
mod arb {
    use libp2p::identity::Keypair;
    use quickcheck::Arbitrary;
    use serde::de::DeserializeOwned;
    use serde::Serialize;

    use crate::arb::ArbSubnetID;

    use super::{SignedVoteRecord, VoteRecord};

    /// Create a valid [`SignedVoteRecord`] with a random key.
    impl<V> Arbitrary for SignedVoteRecord<V>
    where
        V: Arbitrary + Serialize + DeserializeOwned,
    {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let key = Keypair::generate_secp256k1();
            let subnet_id = ArbSubnetID::arbitrary(g).0;
            let content = V::arbitrary(g);

            VoteRecord::signed(&key, subnet_id, content).expect("error creating signed envelope")
        }
    }
}

#[cfg(test)]
mod tests {
    use quickcheck_macros::quickcheck;

    use super::SignedVoteRecord;

    #[quickcheck]
    fn prop_roundtrip(signed_record: SignedVoteRecord<String>) -> bool {
        crate::signed_record::tests::prop_roundtrip(signed_record)
    }
}
