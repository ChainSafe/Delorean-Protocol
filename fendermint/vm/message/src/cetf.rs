// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use fendermint_actor_cetf::BlsSignature;

use serde::{Deserialize, Serialize};

/// Messages involved in Cetf.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum CetfMessage {
    CetfTag(u64, BlsSignature),

    // BlockHeightTag(u64, BlsSignature),
}

#[cfg(feature = "arb")]
mod arb {
    // use quickcheck::{Arbitrary, Gen};

    // use super::CetfMessage;

    // impl Arbitrary for CetfMessage {
    //     fn arbitrary(u: &mut Gen) -> Self {
    //         match u8::arbitrary(u) % 3 {
    //             0 => CetfMessage::CetfTag(u64::arbitrary(u), Vec::arbitrary(u)),
    //             1 => CetfMessage::BlockHashTag(Vec::arbitrary(u), Vec::arbitrary(u)),
    //             _ => CetfMessage::BlockHeightTag(u64::arbitrary(u), Vec::arbitrary(u)),
    //         }
    //     }
    // }
}
