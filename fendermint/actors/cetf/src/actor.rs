// Copyright 2021-2023 BadBoi Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actors_runtime::actor_dispatch;
use fil_actors_runtime::actor_error;
use fil_actors_runtime::builtin::singletons::SYSTEM_ACTOR_ADDR;
use fil_actors_runtime::runtime::{ActorCode, Runtime};
use fil_actors_runtime::ActorError;

use crate::state::State;
use crate::{EnqueueTagParams, GetTagParams};
use crate::{Method, CETF_ACTOR_NAME};

fil_actors_runtime::wasm_trampoline!(Actor);

// fvm_sdk::sys::fvm_syscalls! {
//     module = "cetf_kernel";
//     pub fn enqueue_tag(tag: *const u8, tag_len: u32) -> Result<()>;
// }

pub struct Actor;
impl Actor {
    /// Initialize the HAMT store for tags in the actor state
    /// Callable only by the system actor at genesis
    pub fn constructor(rt: &impl Runtime) -> Result<(), ActorError> {
        println!("cetf actor constructor called");
        rt.validate_immediate_caller_is(std::iter::once(&SYSTEM_ACTOR_ADDR))?;

        let st = State::new(rt.store())?;
        rt.create(&st)?;

        Ok(())
    }

    /// Add a new tag to the state to be signed by the validators
    /// Callable by anyone and designed to be called from Solidity contracts
    pub fn enqueue_tag(rt: &impl Runtime, params: EnqueueTagParams) -> Result<(), ActorError> {
        let state: State = rt.state()?;
        // NOTE: use of epoch is intentional here. In fendermint the epoch is the block height
        state.add_tag_at_height(rt, &rt.curr_epoch(), params.tag)?;

        Ok(())
    }

    /// Clear a tag as presumably it has now been signed by the validators at it corresponding height
    /// Callable only by the system actor
    pub fn get_tag(rt: &impl Runtime, params: GetTagParams) -> Result<(), ActorError> {
        let state: State = rt.state()?;
        state.get_tag_at_height(rt.store(), &params.height)?;
        Ok(())
    }
}

impl ActorCode for Actor {
    type Methods = Method;

    fn name() -> &'static str {
        CETF_ACTOR_NAME
    }

    actor_dispatch! {
        Constructor => constructor,
        EnqueueTag => enqueue_tag,
        GetTag => get_tag,
    }
}
