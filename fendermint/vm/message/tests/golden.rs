// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

/// Examples of `ChainMessage`, which is what the client has to send,
/// or at least what appears in blocks.
mod chain {
    use fendermint_testing::golden_cbor;
    use fendermint_vm_message::{chain::ChainMessage, ipc::IpcMessage};
    use quickcheck::Arbitrary;

    golden_cbor! { "chain", signed, |g| {
        loop {
            if let msg @ ChainMessage::Signed(_) = ChainMessage::arbitrary(g) {
                return msg
            }
        }
      }
    }

    golden_cbor! { "chain", ipc_bottom_up_resolve, |g| {
        loop {
            if let msg @ ChainMessage::Ipc(IpcMessage::BottomUpResolve(_)) = ChainMessage::arbitrary(g) {
                return msg
            }
        }
      }
    }

    golden_cbor! { "chain", ipc_bottom_up_exec, |g| {
        loop {
            if let msg @ ChainMessage::Ipc(IpcMessage::BottomUpExec(_)) = ChainMessage::arbitrary(g) {
                return msg
            }
        }
      }
    }

    golden_cbor! { "chain", ipc_top_down, |g| {
        loop {
            if let msg @ ChainMessage::Ipc(IpcMessage::TopDownExec(_)) = ChainMessage::arbitrary(g) {
                return msg
            }
        }
      }
    }
}

/// Examples of FVM messages, which is what the client needs to sign.
mod fvm {
    use fendermint_testing::golden_cid;
    use fendermint_vm_message::signed::SignedMessage;
    use quickcheck::Arbitrary;

    golden_cid! { "fvm", message, |g| SignedMessage::arbitrary(g).message, |m| SignedMessage::cid(m).unwrap() }
}

/// Examples of query requests the client needs to send, and client responses it will receive.
mod query {
    mod request {
        use fendermint_testing::golden_cbor;
        use fendermint_vm_message::query::FvmQuery;
        use quickcheck::Arbitrary;

        golden_cbor! { "query/request", ipld, |g| {
            loop {
                if let msg @ FvmQuery::Ipld(_) = FvmQuery::arbitrary(g) {
                    return msg
                }
            }
        }}

        golden_cbor! { "query/request", actor_state, |g| {
            loop {
                if let msg @ FvmQuery::ActorState { .. } = FvmQuery::arbitrary(g) {
                    return msg
                }
            }
        }}
    }

    mod response {
        use fendermint_testing::golden_cbor;
        use fendermint_vm_message::query::ActorState;
        use quickcheck::Arbitrary;

        golden_cbor! { "query/response", actor_state, |g| {
            ActorState::arbitrary(g)
        }}
    }
}
