// Copyright 2021-2023 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actors_runtime::actor_dispatch;
use fil_actors_runtime::actor_error;
use fil_actors_runtime::builtin::singletons::SYSTEM_ACTOR_ADDR;
use fil_actors_runtime::runtime::{ActorCode, Runtime};
use fil_actors_runtime::ActorDowncast;
use fil_actors_runtime::ActorError;
use fil_actors_runtime::Array;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::error::ExitCode;

use crate::{
    BlockHash, ConstructorParams, Method, PushBlockParams, State, CHAINMETADATA_ACTOR_NAME,
};

fil_actors_runtime::wasm_trampoline!(Actor);

pub struct Actor;

impl Actor {
    fn constructor(rt: &impl Runtime, params: ConstructorParams) -> Result<(), ActorError> {
        rt.validate_immediate_caller_is(std::iter::once(&SYSTEM_ACTOR_ADDR))?;

        let state = State::new(rt.store(), params.lookback_len).map_err(|e| {
            e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to create empty AMT")
        })?;

        rt.create(&state)?;

        Ok(())
    }

    fn push_block_hash(rt: &impl Runtime, params: PushBlockParams) -> Result<(), ActorError> {
        rt.validate_immediate_caller_is(std::iter::once(&SYSTEM_ACTOR_ADDR))?;

        rt.transaction(|st: &mut State, rt| {
            // load the blockhashes AMT
            let mut blockhashes = Array::load(&st.blockhashes, rt.store()).map_err(|e| {
                e.downcast_default(
                    ExitCode::USR_ILLEGAL_STATE,
                    "failed to load blockhashes states",
                )
            })?;

            // push the block to the AMT
            blockhashes.set(params.epoch as u64, params.block).unwrap();

            // remove the oldest block if the AMT is full (note that this assume the
            // for_each_while iterates in order, which it seems to do)
            if blockhashes.count() > st.lookback_len {
                let mut first_idx = 0;
                blockhashes
                    .for_each_while(|i, _: &BlockHash| {
                        first_idx = i;
                        Ok(false)
                    })
                    .unwrap();
                blockhashes.delete(first_idx).unwrap();
            }

            // save the new blockhashes AMT cid root
            st.blockhashes = blockhashes.flush().map_err(|e| {
                e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to save blockhashes")
            })?;

            Ok(())
        })?;

        Ok(())
    }

    fn lookback_len(rt: &impl Runtime) -> Result<u64, ActorError> {
        let state: State = rt.state()?;
        Ok(state.lookback_len)
    }

    fn get_block_hash(
        rt: &impl Runtime,
        epoch: ChainEpoch,
    ) -> Result<Option<BlockHash>, ActorError> {
        let st: State = rt.state()?;

        st.get_block_hash(rt.store(), epoch)
            .map_err(|e| e.downcast_default(ExitCode::USR_ILLEGAL_STATE, "failed to get blockhash"))
    }
}

impl ActorCode for Actor {
    type Methods = Method;

    fn name() -> &'static str {
        CHAINMETADATA_ACTOR_NAME
    }

    actor_dispatch! {
        Constructor => constructor,
        PushBlockHash => push_block_hash,
        LookbackLen => lookback_len,
        GetBlockHash => get_block_hash,
    }
}
