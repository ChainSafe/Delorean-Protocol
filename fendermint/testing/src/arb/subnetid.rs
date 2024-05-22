// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use fvm_shared::address::Address;
use ipc_api::subnet_id::SubnetID;

#[derive(Debug, Clone)]
pub struct ArbSubnetAddress(pub Address);

#[derive(Debug, Clone)]
pub struct ArbSubnetID(pub SubnetID);

impl quickcheck::Arbitrary for ArbSubnetID {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let child_count = usize::arbitrary(g) % 4;

        let children = (0..child_count)
            .map(|_| ArbSubnetAddress::arbitrary(g).0)
            .collect::<Vec<_>>();

        Self(SubnetID::new(u64::arbitrary(g), children))
    }
}

impl arbitrary::Arbitrary<'_> for ArbSubnetID {
    fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
        let child_count = usize::arbitrary(u)? % 4;

        let children = (0..child_count)
            .map(|_| Ok(ArbSubnetAddress::arbitrary(u)?.0))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self(SubnetID::new(u64::arbitrary(u)?, children)))
    }
}

impl quickcheck::Arbitrary for ArbSubnetAddress {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let addr = if bool::arbitrary(g) {
            Address::new_id(u64::arbitrary(g))
        } else {
            // Only expecting EAM managed delegated addresses.
            let subaddr: [u8; 20] = std::array::from_fn(|_| u8::arbitrary(g));
            Address::new_delegated(10, &subaddr).unwrap()
        };
        Self(addr)
    }
}

impl arbitrary::Arbitrary<'_> for ArbSubnetAddress {
    fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
        let addr = if bool::arbitrary(u)? {
            Address::new_id(u64::arbitrary(u)?)
        } else {
            // Only expecting EAM managed delegated addresses.
            let mut subaddr = [0u8; 20];
            for b in &mut subaddr {
                *b = u8::arbitrary(u)?;
            }
            Address::new_delegated(10, &subaddr).unwrap()
        };
        Ok(Self(addr))
    }
}
