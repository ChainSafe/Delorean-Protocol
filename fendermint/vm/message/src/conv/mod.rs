// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod from_eth;
pub mod from_fvm;

#[cfg(test)]
pub mod tests {
    use fendermint_crypto::{PublicKey, SecretKey};
    use fendermint_testing::arb::{ArbMessage, ArbTokenAmount};
    use fendermint_vm_actor_interface::{
        eam::{self, EthAddress},
        evm,
    };
    use fvm_ipld_encoding::{BytesSer, RawBytes};
    use fvm_shared::{address::Address, bigint::Integer, econ::TokenAmount, message::Message};
    use rand::{rngs::StdRng, SeedableRng};

    use super::from_fvm::MAX_U256;

    #[derive(Clone, Debug)]
    struct EthDelegatedAddress(Address);

    impl quickcheck::Arbitrary for EthDelegatedAddress {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let mut subaddr: [u8; 20] = std::array::from_fn(|_| u8::arbitrary(g));
            while EthAddress(subaddr).is_masked_id() {
                subaddr[0] = u8::arbitrary(g);
            }
            Self(Address::new_delegated(eam::EAM_ACTOR_ID, &subaddr).unwrap())
        }
    }

    #[derive(Clone, Debug)]
    struct EthTokenAmount(TokenAmount);

    impl quickcheck::Arbitrary for EthTokenAmount {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let t = ArbTokenAmount::arbitrary(g).0;
            let (_, t) = t.atto().div_mod_floor(&MAX_U256);
            Self(TokenAmount::from_atto(t))
        }
    }

    /// Message that only contains data which can survive a roundtrip.
    #[derive(Clone, Debug)]
    pub struct EthMessage(pub Message);

    impl quickcheck::Arbitrary for EthMessage {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let mut m = ArbMessage::arbitrary(g).0;
            m.version = 0;
            m.method_num = evm::Method::InvokeContract as u64;
            m.from = EthDelegatedAddress::arbitrary(g).0;
            m.to = EthDelegatedAddress::arbitrary(g).0;
            m.value = EthTokenAmount::arbitrary(g).0;
            m.gas_fee_cap = EthTokenAmount::arbitrary(g).0;
            m.gas_premium = EthTokenAmount::arbitrary(g).0;
            // The random bytes will fail to deserialize.
            // With the EVM we expect them to be IPLD serialized bytes.
            m.params =
                RawBytes::serialize(BytesSer(m.params.bytes())).expect("failedto serialize params");
            Self(m)
        }
    }

    #[derive(Debug, Clone)]
    pub struct KeyPair {
        pub sk: SecretKey,
        pub pk: PublicKey,
    }

    impl quickcheck::Arbitrary for KeyPair {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let seed = u64::arbitrary(g);
            let mut rng = StdRng::seed_from_u64(seed);
            let sk = SecretKey::random(&mut rng);
            let pk = sk.public_key();
            Self { sk, pk }
        }
    }
}
