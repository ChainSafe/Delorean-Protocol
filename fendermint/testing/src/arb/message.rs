// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use fvm_shared::message::Message;
use quickcheck::{Arbitrary, Gen};

use super::{ArbAddress, ArbTokenAmount};

#[derive(Clone, Debug)]
pub struct ArbMessage(pub Message);

impl Arbitrary for ArbMessage {
    fn arbitrary(g: &mut Gen) -> Self {
        let mut message = Message::arbitrary(g);
        message.gas_fee_cap = ArbTokenAmount::arbitrary(g).0;
        message.gas_premium = ArbTokenAmount::arbitrary(g).0;
        message.value = ArbTokenAmount::arbitrary(g).0;
        message.to = ArbAddress::arbitrary(g).0;
        message.from = ArbAddress::arbitrary(g).0;
        Self(message)
    }
}
