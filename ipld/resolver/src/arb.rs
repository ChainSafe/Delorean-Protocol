// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use fvm_shared::address::Address;
use ipc_api::subnet_id::SubnetID;
use libipld::{Cid, Multihash};
use quickcheck::Arbitrary;

/// Unfortunately an arbitrary `DelegatedAddress` can be inconsistent
/// with bytes that do not correspond to its length. This struct fixes
/// that so we can generate arbitrary addresses that don't fail equality
/// after a roundtrip.
#[derive(Clone, Debug)]
pub struct ArbAddress(pub Address);

impl Arbitrary for ArbAddress {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let addr = Address::arbitrary(g);
        let bz = addr.to_bytes();
        let addr = Address::from_bytes(&bz).expect("address roundtrip works");
        Self(addr)
    }
}

#[derive(Clone, Debug)]
pub struct ArbSubnetID(pub SubnetID);

impl Arbitrary for ArbSubnetID {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let child_count = usize::arbitrary(g) % 4;

        let children = (0..child_count)
            .map(|_| {
                if bool::arbitrary(g) {
                    Address::new_id(u64::arbitrary(g))
                } else {
                    // Only expectign EAM managed delegated addresses.
                    let subaddr: [u8; 20] = std::array::from_fn(|_| Arbitrary::arbitrary(g));
                    Address::new_delegated(10, &subaddr).unwrap()
                }
            })
            .collect::<Vec<_>>();

        Self(SubnetID::new(u64::arbitrary(g), children))
    }
}

/// Unfortunately ref-fvm depends on cid:0.8.6, which depends on quickcheck:0.9
/// whereas here we use quickcheck:1.0. This causes conflicts and the `Arbitrary`
/// implementations for `Cid` are not usable to us, nor can we patch all `cid`
/// dependencies to use 0.9 because then the IPLD and other FVM traits don't work.
///
/// TODO: Remove this module when the `cid` dependency is updated.
///
/// NOTE: This is based on the [simpler version](https://github.com/ChainSafe/forest/blob/v0.6.0/blockchain/blocks/src/lib.rs) in Forest.
///       The original uses weighted distributions to generate more plausible CIDs.
#[derive(Clone)]
pub struct ArbCid(pub Cid);

impl Arbitrary for ArbCid {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Self(Cid::new_v1(
            u64::arbitrary(g),
            Multihash::wrap(u64::arbitrary(g), &[u8::arbitrary(g)]).unwrap(),
        ))
    }
}
