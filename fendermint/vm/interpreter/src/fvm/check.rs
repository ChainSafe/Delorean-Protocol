// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use async_trait::async_trait;

use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::{address::Address, error::ExitCode};

use crate::CheckInterpreter;

use super::{state::FvmExecState, store::ReadOnlyBlockstore, FvmMessage, FvmMessageInterpreter};

/// Transaction check results are expressed by the exit code, so that hopefully
/// they would result in the same error code if they were applied.
pub struct FvmCheckRet {
    pub sender: Address,
    pub gas_limit: u64,
    pub exit_code: ExitCode,
    pub return_data: Option<RawBytes>,
    pub info: Option<String>,
}

#[async_trait]
impl<DB, TC> CheckInterpreter for FvmMessageInterpreter<DB, TC>
where
    DB: Blockstore + 'static + Send + Sync + Clone,
    TC: Send + Sync + 'static,
{
    // We simulate the full pending state so that client can call methods on
    // contracts that haven't been deployed yet.
    type State = FvmExecState<ReadOnlyBlockstore<DB>>;
    type Message = FvmMessage;
    type Output = FvmCheckRet;

    /// Check that:
    /// * sender exists
    /// * sender nonce matches the message sequence
    /// * sender has enough funds to cover the gas cost
    async fn check(
        &self,
        mut state: Self::State,
        msg: Self::Message,
        _is_recheck: bool,
    ) -> anyhow::Result<(Self::State, Self::Output)> {
        let checked = |state,
                       exit_code: ExitCode,
                       gas_used: Option<u64>,
                       return_data: Option<RawBytes>,
                       info: Option<String>| {
            tracing::info!(
                exit_code = exit_code.value(),
                from = msg.from.to_string(),
                to = msg.to.to_string(),
                method_num = msg.method_num,
                gas_limit = msg.gas_limit,
                gas_used = gas_used.unwrap_or_default(),
                info = info.clone().unwrap_or_default(),
                "check transaction"
            );
            let ret = FvmCheckRet {
                sender: msg.from,
                gas_limit: msg.gas_limit,
                exit_code,
                return_data,
                info,
            };
            Ok((state, ret))
        };

        if let Err(e) = msg.check() {
            return checked(
                state,
                ExitCode::SYS_ASSERTION_FAILED,
                None,
                None,
                Some(format!("pre-check failure: {:#}", e)),
            );
        }

        // NOTE: This would be a great place for let-else, but clippy runs into a compilation bug.
        let state_tree = state.state_tree_mut();

        // This code is left in place for reference of a partial check performed on top of `FvmCheckState`.
        if let Some(id) = state_tree.lookup_id(&msg.from)? {
            if let Some(mut actor) = state_tree.get_actor(id)? {
                let balance_needed = msg.gas_fee_cap.clone() * msg.gas_limit;
                if actor.balance < balance_needed {
                    return checked(
                        state,
                        ExitCode::SYS_SENDER_STATE_INVALID,
                        None,
                        None,
                        Some(
                            format! {"actor balance {} less than needed {}", actor.balance, balance_needed},
                        ),
                    );
                } else if actor.sequence != msg.sequence {
                    return checked(
                        state,
                        ExitCode::SYS_SENDER_STATE_INVALID,
                        None,
                        None,
                        Some(
                            format! {"expected sequence {}, got {}", actor.sequence, msg.sequence},
                        ),
                    );
                } else if self.exec_in_check {
                    // Instead of modifying just the partial state, we will execute the call in earnest.
                    // This is required for fully supporting the Ethereum API "pending" queries, if that's needed.

                    // This will stack the effect for subsequent transactions added to the mempool.
                    let (apply_ret, _) = state.execute_explicit(msg.clone())?;

                    return checked(
                        state,
                        apply_ret.msg_receipt.exit_code,
                        Some(apply_ret.msg_receipt.gas_used),
                        Some(apply_ret.msg_receipt.return_data),
                        apply_ret
                            .failure_info
                            .map(|i| i.to_string())
                            .filter(|s| !s.is_empty()),
                    );
                } else {
                    actor.sequence += 1;
                    actor.balance -= balance_needed;
                    state_tree.set_actor(id, actor);

                    return checked(state, ExitCode::OK, None, None, None);
                }
            }
        }

        checked(
            state,
            ExitCode::SYS_SENDER_INVALID,
            None,
            None,
            Some(format! {"cannot find actor {}", msg.from}),
        )
    }
}
