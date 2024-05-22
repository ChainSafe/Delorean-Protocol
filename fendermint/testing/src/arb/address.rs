// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use fvm_shared::address::Address;
use quickcheck::{Arbitrary, Gen};

/// Unfortunately an arbitrary `DelegatedAddress` can be inconsistent with bytes that do not correspond to its length.
#[derive(Clone, Debug)]
pub struct ArbAddress(pub Address);

impl Arbitrary for ArbAddress {
    fn arbitrary(g: &mut Gen) -> Self {
        let addr = Address::arbitrary(g);
        let bz = addr.to_bytes();
        Self(Address::from_bytes(&bz).unwrap())
    }
}
