// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT

use libp2p::core::signed_envelope;
use libp2p::identity::PublicKey;
use libp2p::{core::SignedEnvelope, identity::Keypair};
use serde::de::DeserializeOwned;
use serde::Serialize;

const DOMAIN_SEP: &str = "/ipc/ipld/resolver";

pub trait Record {
    /// Payload type for the [`SignedEnvelope`].
    fn payload_type() -> &'static str;
    /// Check that the [`PublicKey`] recovered from the [`SignedEnvelope`]
    /// is consistent with the payload.
    fn check_signing_key(&self, key: &PublicKey) -> bool;
}

/// A [`ProviderRecord`] with a [`SignedEnvelope`] proving that the
/// peer indeed is ready to provide the data for the listed subnets.
#[derive(Debug, Clone)]
pub struct SignedRecord<R> {
    /// The deserialized and validated record.
    record: R,
    /// The [`SignedEnvelope`] from which the record was deserialized from.
    envelope: SignedEnvelope,
}

// Based on `libp2p_core::peer_record::PeerRecord`
impl<R> SignedRecord<R>
where
    R: Record + Serialize + DeserializeOwned,
{
    /// Create a new [`SignedRecord`] with a signed envelope
    /// which can be shared with others.
    pub fn new(key: &Keypair, record: R) -> anyhow::Result<Self> {
        let payload = fvm_ipld_encoding::to_vec(&record)?;
        let envelope = SignedEnvelope::new(
            key,
            DOMAIN_SEP.to_owned(),
            R::payload_type().as_bytes().to_vec(),
            payload,
        )?;
        Ok(Self { record, envelope })
    }

    pub fn from_signed_envelope(envelope: SignedEnvelope) -> Result<Self, FromEnvelopeError> {
        let (payload, signing_key) = envelope
            .payload_and_signing_key(DOMAIN_SEP.to_owned(), R::payload_type().as_bytes())?;

        let record = fvm_ipld_encoding::from_slice::<R>(payload)?;

        if !record.check_signing_key(signing_key) {
            return Err(FromEnvelopeError::MismatchedSignature);
        }

        Ok(Self { record, envelope })
    }

    /// Deserialize then check the domain tags and the signature.
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        let envelope = SignedEnvelope::from_protobuf_encoding(bytes)?;
        let signed_record = Self::from_signed_envelope(envelope)?;
        Ok(signed_record)
    }

    pub fn record(&self) -> &R {
        &self.record
    }

    pub fn envelope(&self) -> &SignedEnvelope {
        &self.envelope
    }

    pub fn into_record(self) -> R {
        self.record
    }

    pub fn into_envelope(self) -> SignedEnvelope {
        self.envelope
    }
}

#[derive(thiserror::Error, Debug)]
pub enum FromEnvelopeError {
    /// Failed to extract the payload from the envelope.
    #[error("Failed to extract payload from envelope")]
    BadPayload(#[from] signed_envelope::ReadPayloadError),
    /// Failed to decode the provided bytes as the record type.
    #[error("Failed to decode bytes as record")]
    InvalidRecord(#[from] fvm_ipld_encoding::Error),
    /// The signer of the envelope is different than the peer id in the record.
    #[error("The signer of the envelope is different than the peer id in the record")]
    MismatchedSignature,
}

#[cfg(test)]
pub mod tests {
    use fvm_ipld_encoding::de::DeserializeOwned;
    use libp2p::core::SignedEnvelope;
    use serde::Serialize;

    use super::{Record, SignedRecord};

    pub fn prop_roundtrip<R>(signed_record: SignedRecord<R>) -> bool
    where
        R: Serialize + DeserializeOwned + Record + PartialEq,
    {
        let envelope_bytes = signed_record.envelope().clone().into_protobuf_encoding();

        let envelope =
            SignedEnvelope::from_protobuf_encoding(&envelope_bytes).expect("envelope roundtrip");

        let signed_record2 =
            SignedRecord::<R>::from_signed_envelope(envelope).expect("record roundtrip");

        signed_record2.into_record() == *signed_record.record()
    }
}
