// Copyright 2022-2024 Protocol Labs
// Copyright 2021-2023 BadBoi Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actors_runtime::actor_dispatch;
use fil_actors_runtime::actor_error;
use fil_actors_runtime::builtin::singletons::SYSTEM_ACTOR_ADDR;
use fil_actors_runtime::runtime::{ActorCode, Runtime};
use fil_actors_runtime::ActorError;
use fvm_shared::event::ActorEvent;

use crate::state::State;
use crate::AddValidatorParams;
use crate::{EnqueueTagParams, GetTagParams};
use crate::{Method, CETF_ACTOR_NAME};

fil_actors_runtime::wasm_trampoline!(Actor);

pub struct Actor;
impl Actor {
    /// Initialize the HAMT store for tags in the actor state
    /// Callable only by the system actor at genesis
    pub fn constructor(rt: &impl Runtime) -> Result<(), ActorError> {
        rt.validate_immediate_caller_is(std::iter::once(&SYSTEM_ACTOR_ADDR))?;

        let st = State::new(rt.store())?;
        rt.create(&st)?;
        Ok(())
    }

    /// Add a new tag to the state to be signed by the validators
    /// Callable by anyone and designed to be called from Solidity contracts
    pub fn enqueue_tag(rt: &impl Runtime, params: EnqueueTagParams) -> Result<(), ActorError> {
        rt.validate_immediate_caller_accept_any()?;

        log::info!(
            "cetf actor enqueue_tag called by {} with tag {:?}",
            rt.message().caller(),
            params.tag
        );

        rt.transaction(|st: &mut State, rt| {
            // if st.enabled == false {
            //     return Err(ActorError::forbidden(
            //         "CETF actor is disabled. Not all validators have added their keys.".to_owned(),
            //     ));
            // }
            // NOTE: use of epoch is intentional here. In fendermint the epoch is the block height
            st.add_tag_at_height(rt, &rt.curr_epoch(), &params.tag)?;
            Ok(())
        })?;

        Ok(())
    }

    /// Clear a tag as presumably it has now been signed by the validators at it corresponding height
    /// Callable only by the system actor
    pub fn get_tag(rt: &impl Runtime, params: GetTagParams) -> Result<(), ActorError> {
        let state: State = rt.state()?;
        state.get_tag_at_height(rt.store(), &params.height)?;
        Ok(())
    }

    pub fn enable(rt: &impl Runtime) -> Result<(), ActorError> {
        rt.validate_immediate_caller_is(std::iter::once(&SYSTEM_ACTOR_ADDR))?;
        rt.transaction(|st: &mut State, rt| {
            st.enabled = true;
            Ok(())
        })?;
        Ok(())
    }

    pub fn disable(rt: &impl Runtime) -> Result<(), ActorError> {
        rt.validate_immediate_caller_is(std::iter::once(&SYSTEM_ACTOR_ADDR))?;
        rt.transaction(|st: &mut State, rt| {
            st.enabled = false;
            Ok(())
        })?;
        Ok(())
    }

    pub fn add_validator(rt: &impl Runtime, params: AddValidatorParams) -> Result<(), ActorError> {
        rt.validate_immediate_caller_is(std::iter::once(&SYSTEM_ACTOR_ADDR))?;
        rt.transaction(|st: &mut State, rt| {
            st.add_validator(rt.store(), &params.address, &params.public_key)?;
            Ok(())
        })?;
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
        Enable => enable,
        AddValidator => add_validator,
        Disable => disable,
    }
}
