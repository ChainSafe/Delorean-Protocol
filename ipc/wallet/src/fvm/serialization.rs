// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod json {
    use base64::{prelude::BASE64_STANDARD, Engine};
    use fvm_shared::crypto::signature::{Signature, SignatureType};
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    // Wrapper for serializing and deserializing a Signature from JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct SignatureJson(#[serde(with = "self")] pub Signature);

    /// Wrapper for serializing a Signature reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct SignatureJsonRef<'a>(#[serde(with = "self")] pub &'a Signature);

    #[derive(Serialize, Deserialize)]
    struct JsonHelper {
        #[serde(rename = "Type")]
        sig_type: SignatureType,
        #[serde(rename = "Data")]
        bytes: String,
    }

    pub fn serialize<S>(m: &Signature, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            sig_type: m.signature_type(),
            bytes: BASE64_STANDARD.encode(&m.bytes),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Signature, D::Error>
    where
        D: Deserializer<'de>,
    {
        let JsonHelper { sig_type, bytes } = Deserialize::deserialize(deserializer)?;
        Ok(Signature {
            sig_type,
            bytes: BASE64_STANDARD.decode(bytes).map_err(de::Error::custom)?,
        })
    }

    #[allow(dead_code)]
    pub mod opt {
        use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

        use super::{Signature, SignatureJson, SignatureJsonRef};

        pub fn serialize<S>(v: &Option<Signature>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            v.as_ref().map(SignatureJsonRef).serialize(serializer)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Signature>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let s: Option<SignatureJson> = Deserialize::deserialize(deserializer)?;
            Ok(s.map(|v| v.0))
        }
    }

    pub mod signature_type {
        use serde::{Deserialize, Deserializer, Serialize, Serializer};

        use super::*;

        #[derive(Debug, Deserialize, Serialize)]
        #[serde(rename_all = "lowercase")]
        enum JsonHelperEnum {
            Bls,
            Secp256k1,
        }

        #[derive(Debug, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct SignatureTypeJson(#[serde(with = "self")] pub SignatureType);

        pub fn serialize<S>(m: &SignatureType, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let json = match *m {
                SignatureType::BLS => JsonHelperEnum::Bls,
                SignatureType::Secp256k1 => JsonHelperEnum::Secp256k1,
            };
            json.serialize(serializer)
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<SignatureType, D::Error>
        where
            D: Deserializer<'de>,
        {
            let json_enum: JsonHelperEnum = Deserialize::deserialize(deserializer)?;

            let signature_type = match json_enum {
                JsonHelperEnum::Bls => SignatureType::BLS,
                JsonHelperEnum::Secp256k1 => SignatureType::Secp256k1,
            };
            Ok(signature_type)
        }
    }
}

#[cfg(test)]
mod tests {
    use fvm_shared::crypto::signature::{Signature, SignatureType};
    use quickcheck_macros::quickcheck;

    use super::json::{signature_type::SignatureTypeJson, SignatureJson, SignatureJsonRef};

    // Auxiliary impl to support quickcheck
    #[derive(Clone, Debug, PartialEq, Eq, Copy)]
    struct SigTypeWrapper(SignatureType);

    impl quickcheck::Arbitrary for SigTypeWrapper {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let sig_type = match u8::arbitrary(g) % 2 {
                0 => SignatureType::BLS,
                1 => SignatureType::Secp256k1,
                _ => unreachable!(),
            };
            SigTypeWrapper(sig_type)
        }
    }

    // Auxiliary impl to support quickcheck
    #[derive(Clone, Debug, PartialEq)]
    struct SignatureWrapper(Signature);

    impl quickcheck::Arbitrary for SignatureWrapper {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let sig_type = SigTypeWrapper::arbitrary(g).0;
            let bytes = Vec::<u8>::arbitrary(g);
            SignatureWrapper(Signature { sig_type, bytes })
        }
    }

    #[quickcheck]
    fn signature_roundtrip(signature: SignatureWrapper) {
        let signature = signature.0;
        let serialized = serde_json::to_string(&SignatureJsonRef(&signature)).unwrap();
        let parsed: SignatureJson = serde_json::from_str(&serialized).unwrap();
        assert_eq!(signature, parsed.0);
    }

    #[quickcheck]
    fn signaturetype_roundtrip(sigtype: SigTypeWrapper) {
        let sigtype = sigtype.0;
        let serialized = serde_json::to_string(&SignatureTypeJson(sigtype)).unwrap();
        let parsed: SignatureTypeJson = serde_json::from_str(&serialized).unwrap();
        assert_eq!(sigtype, parsed.0);
    }
}
