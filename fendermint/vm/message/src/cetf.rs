// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use fendermint_actor_cetf::{BlockHash, BlsSignature};
use fvm_shared::{
    address::Address, clock::ChainEpoch, crypto::signature::Signature, econ::TokenAmount,
};
use ipc_api::subnet_id::SubnetID;
use serde::{Deserialize, Serialize};

/// Messages involved in InterPlanetary Consensus.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum CetfMessage {
    /// A bottom-up checkpoint coming from a child subnet "for resolution", relayed by a user of the parent subnet for a reward.
    ///
    /// The reward can be given immediately upon the validation of the quorum certificate in the checkpoint,
    /// or later during execution, once data availability has been confirmed.
    CetfTag(u64, BlsSignature),

    /// A bottom-up checkpoint proposed "for execution" by the parent subnet validators, provided that the majority of them
    /// have the data available to them, already resolved.
    ///
    /// To prove that the data is available, we can either use the ABCI++ "process proposal" mechanism,
    /// or we can gossip votes using the _IPLD Resolver_ and attach them as a quorum certificate.
    BlockHashTag(BlockHash, BlsSignature),

    /// A top-down checkpoint parent finality proposal. This proposal should contain the latest parent
    /// state that to be checked and voted by validators.
    BlockHeightTag(u64, BlsSignature),
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
