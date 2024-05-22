// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
//! Cross network messages related struct and utility functions.

use crate::address::IPCAddress;
use crate::subnet_id::SubnetID;
use crate::HumanReadable;
use anyhow::anyhow;
use fvm_shared::address::Address;
use fvm_shared::econ::TokenAmount;
use serde::{Deserialize, Serialize};
use serde_tuple::{Deserialize_tuple, Serialize_tuple};
use serde_with::serde_as;

#[serde_as]
#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct IpcEnvelope {
    /// Type of message being propagated.
    pub kind: IpcMsgKind,
    /// destination of the message
    /// It makes sense to extract from the encoded message
    /// all shared fields required by all message, so they
    /// can be inspected without having to decode the message.
    #[serde_as(as = "HumanReadable")]
    pub to: IPCAddress,
    /// Value included in the envelope
    pub value: TokenAmount,
    /// address sending the message
    pub from: IPCAddress,
    /// abi.encoded message
    #[serde_as(as = "HumanReadable")]
    pub message: Vec<u8>,
    /// outgoing nonce for the envelope.
    /// This nonce is set by the gateway when committing the message for propagation
    pub nonce: u64,
}

impl IpcEnvelope {
    pub fn new_release_msg(
        sub_id: &SubnetID,
        from: &Address,
        to: &Address,
        value: TokenAmount,
    ) -> anyhow::Result<Self> {
        let to = IPCAddress::new(
            &match sub_id.parent() {
                Some(s) => s,
                None => return Err(anyhow!("error getting parent for subnet addr")),
            },
            to,
        )?;

        let from = IPCAddress::new(sub_id, from)?;
        Ok(Self {
            kind: IpcMsgKind::Transfer,
            from,
            to,
            value,
            nonce: 0,
            message: Default::default(),
        })
    }

    pub fn new_fund_msg(
        sub_id: &SubnetID,
        from: &Address,
        to: &Address,
        value: TokenAmount,
    ) -> anyhow::Result<Self> {
        let from = IPCAddress::new(
            &match sub_id.parent() {
                Some(s) => s,
                None => return Err(anyhow!("error getting parent for subnet addr")),
            },
            from,
        )?;
        let to = IPCAddress::new(sub_id, to)?;

        // the nonce and the rest of message fields are set when the message is committed.
        Ok(Self {
            kind: IpcMsgKind::Transfer,
            from,
            to,
            value,
            nonce: 0,
            message: Default::default(),
        })
    }

    pub fn ipc_type(&self) -> anyhow::Result<IPCMsgType> {
        let sto = self.to.subnet()?;
        let sfrom = self.from.subnet()?;
        if is_bottomup(&sfrom, &sto) {
            return Ok(IPCMsgType::BottomUp);
        }
        Ok(IPCMsgType::TopDown)
    }

    pub fn apply_type(&self, curr: &SubnetID) -> anyhow::Result<IPCMsgType> {
        let sto = self.to.subnet()?;
        let sfrom = self.from.subnet()?;
        if curr.common_parent(&sto) == sfrom.common_parent(&sto)
            && self.ipc_type()? == IPCMsgType::BottomUp
        {
            return Ok(IPCMsgType::BottomUp);
        }
        Ok(IPCMsgType::TopDown)
    }
}

/// Type of cross-net messages currently supported
#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize, strum::Display)]
#[repr(u8)]
pub enum IpcMsgKind {
    /// for cross-net messages that move native token, i.e. fund/release.
    /// and in the future multi-level token transactions.
    Transfer,
    /// general-purpose cross-net transaction that call smart contracts.
    Call,
    /// receipt from the execution of cross-net messages
    /// (currently limited to `Transfer` messages)
    Receipt,
}

impl TryFrom<u8> for IpcMsgKind {
    type Error = anyhow::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => IpcMsgKind::Transfer,
            1 => IpcMsgKind::Call,
            2 => IpcMsgKind::Receipt,
            _ => return Err(anyhow!("invalid ipc msg kind")),
        })
    }
}

#[derive(PartialEq, Eq)]
pub enum IPCMsgType {
    BottomUp,
    TopDown,
}

pub fn is_bottomup(from: &SubnetID, to: &SubnetID) -> bool {
    let index = match from.common_parent(to) {
        Some((ind, _)) => ind,
        None => return false,
    };
    // more children than the common parent
    from.children_as_ref().len() > index
}

#[derive(PartialEq, Eq, Clone, Debug, Default, Serialize_tuple, Deserialize_tuple)]
pub struct CrossMsgs {
    // FIXME: Consider to make this an AMT if we expect
    // a lot of cross-messages to be propagated.
    pub msgs: Vec<IpcEnvelope>,
}

#[derive(Serialize_tuple, Deserialize_tuple, Clone)]
struct ApplyMsgParams {
    pub cross_msg: IpcEnvelope,
}

impl CrossMsgs {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(feature = "fil-actor")]
impl IpcEnvelope {
    pub fn send(
        self,
        rt: &impl fil_actors_runtime::runtime::Runtime,
        rto: &Address,
    ) -> Result<RawBytes, fil_actors_runtime::ActorError> {
        let blk = if !self.wrapped {
            let msg = self.msg;
            rt.send(rto, msg.method, msg.params.into(), msg.value)?
        } else {
            let method = self.msg.method;
            let value = self.msg.value.clone();
            let params =
                fvm_ipld_encoding::ipld_block::IpldBlock::serialize_cbor(&ApplyMsgParams {
                    cross_msg: self,
                })?;
            rt.send(rto, method, params, value)?
        };

        Ok(match blk {
            Some(b) => b.data.into(), // FIXME: this assumes cbor serialization. We should maybe return serialized IpldBlock
            None => RawBytes::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::cross::*;
    use std::str::FromStr;

    #[test]
    fn test_is_bottomup() {
        bottom_up("/r123/f01", "/r123/f01/f02", false);
        bottom_up("/r123/f01", "/r123", true);
        bottom_up("/r123/f01", "/r123/f01/f02", false);
        bottom_up("/r123/f01", "/r123/f02/f02", true);
        bottom_up("/r123/f01/f02", "/r123/f01/f02", false);
        bottom_up("/r123/f01/f02", "/r123/f01/f02/f03", false);
    }

    fn bottom_up(a: &str, b: &str, res: bool) {
        assert_eq!(
            is_bottomup(
                &SubnetID::from_str(a).unwrap(),
                &SubnetID::from_str(b).unwrap()
            ),
            res
        );
    }
}
