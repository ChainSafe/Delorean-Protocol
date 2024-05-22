// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
//! Unfortunately ref-fvm depends on cid:0.8.6, which depends on quickcheck:0.9
//! whereas here we use quickcheck:0.1. This causes conflicts and the `Arbitrary`
//! implementations for `Cid` are not usable to us, nor can we patch all `cid`
//! dependencies to use 0.9 because then the IPLD and other FVM traits don't work.
//!
//! TODO: Remove this module when the `cid` dependency is updated.
//! NOTE: A simpler alternative is https://github.com/ChainSafe/forest/blob/v0.6.0/blockchain/blocks/src/lib.rs
use cid::{
    multihash::{Code, MultihashDigest, MultihashGeneric},
    CidGeneric, Version,
};
use rand::{distributions::WeightedIndex, prelude::Distribution, Rng, RngCore, SeedableRng};

use quickcheck::{Arbitrary, Gen};

#[derive(Clone)]
pub struct ArbVersion(pub Version);

impl Arbitrary for ArbVersion {
    fn arbitrary(g: &mut Gen) -> Self {
        let version = u64::from(bool::arbitrary(g));
        Self(Version::try_from(version).unwrap())
    }
}

#[derive(Clone)]
pub struct ArbCid<const S: usize>(pub CidGeneric<S>);

impl<const S: usize> Arbitrary for ArbCid<S> {
    /// Copied from https://github.com/multiformats/rust-cid/blob/v0.10.0/src/arb.rs
    fn arbitrary(g: &mut Gen) -> Self {
        let cid = if S >= 32 && ArbVersion::arbitrary(g).0 == Version::V0 {
            let data: Vec<u8> = Vec::arbitrary(g);
            let hash = Code::Sha2_256
                .digest(&data)
                .resize()
                .expect("digest too large");
            CidGeneric::new_v0(hash).expect("sha2_256 is a valid hash for cid v0")
        } else {
            // In real world lower IPLD Codec codes more likely to happen, hence distribute them
            // with bias towards smaller values.
            let weights = [128, 32, 4, 4, 2, 2, 1, 1];
            let dist = WeightedIndex::new(weights.iter()).unwrap();
            let mut rng = rand::rngs::SmallRng::seed_from_u64(u64::arbitrary(g));
            let codec = match dist.sample(&mut rng) {
                0 => rng.gen_range(0..u64::pow(2, 7)),
                1 => rng.gen_range(u64::pow(2, 7)..u64::pow(2, 14)),
                2 => rng.gen_range(u64::pow(2, 14)..u64::pow(2, 21)),
                3 => rng.gen_range(u64::pow(2, 21)..u64::pow(2, 28)),
                4 => rng.gen_range(u64::pow(2, 28)..u64::pow(2, 35)),
                5 => rng.gen_range(u64::pow(2, 35)..u64::pow(2, 42)),
                6 => rng.gen_range(u64::pow(2, 42)..u64::pow(2, 49)),
                7 => rng.gen_range(u64::pow(2, 56)..u64::pow(2, 63)),
                _ => unreachable!(),
            };

            let hash = ArbMultihash::<S>::arbitrary(g).0;
            CidGeneric::new_v1(codec, hash)
        };
        Self(cid)
    }
}

#[derive(Clone)]
pub struct ArbMultihash<const S: usize>(pub MultihashGeneric<S>);

impl<const S: usize> Arbitrary for ArbMultihash<S> {
    /// Generates a random valid multihash.
    ///
    /// Copied from https://github.com/multiformats/rust-multihash/blob/v0.18.0/src/arb.rs
    fn arbitrary(g: &mut Gen) -> Self {
        // In real world lower multihash codes are more likely to happen, hence distribute them
        // with bias towards smaller values.
        let weights = [128, 64, 32, 16, 8, 4, 2, 1];
        let dist = WeightedIndex::new(weights.iter()).unwrap();
        let mut rng = rand::rngs::SmallRng::seed_from_u64(u64::arbitrary(g));
        let code = match dist.sample(&mut rng) {
            0 => rng.gen_range(0..u64::pow(2, 7)),
            1 => rng.gen_range(u64::pow(2, 7)..u64::pow(2, 14)),
            2 => rng.gen_range(u64::pow(2, 14)..u64::pow(2, 21)),
            3 => rng.gen_range(u64::pow(2, 21)..u64::pow(2, 28)),
            4 => rng.gen_range(u64::pow(2, 28)..u64::pow(2, 35)),
            5 => rng.gen_range(u64::pow(2, 35)..u64::pow(2, 42)),
            6 => rng.gen_range(u64::pow(2, 42)..u64::pow(2, 49)),
            7 => rng.gen_range(u64::pow(2, 56)..u64::pow(2, 63)),
            _ => unreachable!(),
        };

        // Maximum size is S byte due to the generic.
        let size = rng.gen_range(0..S);
        let mut data = [0; S];
        rng.fill_bytes(&mut data);
        Self(MultihashGeneric::wrap(code, &data[..size]).unwrap())
    }
}
