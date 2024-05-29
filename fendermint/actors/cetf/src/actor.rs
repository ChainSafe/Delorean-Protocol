// Copyright 2022-2024 Protocol Labs
// Copyright 2021-2023 BadBoi Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::state::State;
use crate::AddSignedBlockHashTagParams;
use crate::AddSignedBlockHeightTagParams;
use crate::AddSignedTagParams;
use crate::AddValidatorParams;
use crate::{EnqueueTagParams, GetTagParams};
use crate::{Method, CETF_ACTOR_NAME};
use fil_actors_runtime::actor_dispatch;
use fil_actors_runtime::actor_error;
use fil_actors_runtime::builtin::singletons::SYSTEM_ACTOR_ADDR;
use fil_actors_runtime::runtime::{ActorCode, Runtime};
use fil_actors_runtime::ActorError;
use fvm_shared::ActorID;

// Note for myself: trampoline initializes a logger if debug mode is enabled.
fil_actors_runtime::wasm_trampoline!(Actor);

pub struct Actor;
impl Actor {
    /// Initialize the HAMT store for tags in the actor state
    /// Callable only by the system actor at genesis
    pub fn constructor(rt: &impl Runtime) -> Result<(), ActorError> {
        rt.validate_immediate_caller_is(std::iter::once(&SYSTEM_ACTOR_ADDR))?;
        log::info!("cetf actor constructor called");
        let st = State::new(rt.store())?;
        rt.create(&st)?;
        Ok(())
    }

    pub fn echo(rt: &impl Runtime) -> Result<(), ActorError> {
        log::info!("echo called");
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
            if st.enabled {
                // NOTE: use of epoch is intentional here. In fendermint the epoch is the block height
                st.add_tag_at_height(rt, &(rt.curr_epoch() as u64), &params.tag)?;
            } else {
                log::info!("CETF actor is disabled. Not all validators have added their keys. No tag was enqueued.");
            }
            Ok(())
        })?;

        Ok(())
    }

    /// Clear a tag as presumably it has now been signed by the validators at it corresponding height
    /// Callable only by the system actor
    pub fn get_tag(rt: &impl Runtime, params: GetTagParams) -> Result<(), ActorError> {
        log::info!("get_tag called");
        rt.validate_immediate_caller_accept_any()?;

        let state: State = rt.state()?;
        state.get_tag_at_height(rt.store(), &params.height)?;
        Ok(())
    }

    pub fn enable(rt: &impl Runtime) -> Result<(), ActorError> {
        log::info!("enable called");
        rt.validate_immediate_caller_is(std::iter::once(&SYSTEM_ACTOR_ADDR))?;
        rt.transaction(|st: &mut State, _rt| {
            st.enabled = true;
            Ok(())
        })?;
        Ok(())
    }

    // TODO: Should be unused for now. Need to figure out the mechanics for validator set changes. Assume static.
    pub fn disable(rt: &impl Runtime) -> Result<(), ActorError> {
        log::info!("disable called");
        rt.validate_immediate_caller_is(std::iter::once(&SYSTEM_ACTOR_ADDR))?;
        rt.transaction(|st: &mut State, _rt| {
            st.enabled = false;
            Ok(())
        })?;
        Ok(())
    }

    // TODO: We should use message.sender instead of having the address as a parameter.
    // Leaving this as is for now because its just easier to write scripts for testing because we can send from the same sender.
    pub fn add_validator(rt: &impl Runtime, params: AddValidatorParams) -> Result<(), ActorError> {
        log::info!(
            "add_validator called with caller: {}",
            rt.message().caller()
        );
        rt.validate_immediate_caller_accept_any()?;

        rt.transaction(|st: &mut State, rt| {
            st.add_validator(rt.store(), &params.address, &params.public_key)?;
            Ok(())
        })?;
        Ok(())
    }

    pub fn add_signed_tag(rt: &impl Runtime, params: AddSignedTagParams) -> Result<(), ActorError> {
        // TODO: Probaby want to restrict this to validators only or something
        log::info!("add_signed_tag called");
        rt.validate_immediate_caller_accept_any()?;

        rt.transaction(|st: &mut State, rt| {
            st.add_signed_tag_at_height(rt, &params.height, &params.signature)?;
            Ok(())
        })?;
        Ok(())
    }

    pub fn add_signed_blockheight_tag(
        rt: &impl Runtime,
        params: AddSignedBlockHeightTagParams,
    ) -> Result<(), ActorError> {
        // TODO: Probaby want to restrict this to validators only or something
        log::info!("add_signed_blockheight_tag called");
        rt.validate_immediate_caller_accept_any()?;
        rt.transaction(|st: &mut State, rt| {
            st.add_signed_blockheight_tag_at_height(rt, &params.height, &params.signature)?;
            Ok(())
        })?;
        Ok(())
    }

    pub fn add_signed_blockhash_tag(
        rt: &impl Runtime,
        params: AddSignedBlockHashTagParams,
    ) -> Result<(), ActorError> {
        // TODO: Probaby want to restrict this to validators only or something
        log::info!("add_signed_blockhash_tag called");
        rt.validate_immediate_caller_accept_any()?;
        rt.transaction(|st: &mut State, rt| {
            st.add_signed_blockhash_tag_at_height(rt, &params.hash, &params.signature)?;
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
        Echo => echo,
        EnqueueTag => enqueue_tag,
        GetTag => get_tag,
        Enable => enable,
        AddValidator => add_validator,
        Disable => disable,
        AddSignedTag => add_signed_tag,
        AddSignedBlockHeightTag => add_signed_blockheight_tag,
        AddSignedBlockHashTag => add_signed_blockhash_tag,
    }
}
