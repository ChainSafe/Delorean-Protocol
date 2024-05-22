// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use fendermint_testing::arb::{ArbCid, ArbTokenAmount};
use fendermint_vm_core::{chainid, Timestamp};
use fvm_shared::version::NetworkVersion;
use quickcheck::{Arbitrary, Gen};

use crate::fvm::state::FvmStateParams;

impl Arbitrary for FvmStateParams {
    fn arbitrary(g: &mut Gen) -> Self {
        Self {
            state_root: ArbCid::arbitrary(g).0,
            timestamp: Timestamp(u64::arbitrary(g)),
            network_version: NetworkVersion::new(*g.choose(&[21]).unwrap()),
            base_fee: ArbTokenAmount::arbitrary(g).0,
            circ_supply: ArbTokenAmount::arbitrary(g).0,
            chain_id: chainid::from_str_hashed(String::arbitrary(g).as_str())
                .unwrap()
                .into(),
            power_scale: *g.choose(&[-1, 0, 3]).unwrap(),
            app_version: *g.choose(&[0, 1, 2]).unwrap(),
        }
    }
}
