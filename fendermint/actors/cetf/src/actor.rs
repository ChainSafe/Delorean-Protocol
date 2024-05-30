// Copyright 2022-2024 Protocol Labs
// Copyright 2021-2023 BadBoi Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use core::hash;

use crate::state::State;
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
use sha3::{Digest, Keccak256};

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

    pub fn echo(rt: &impl Runtime, _params: ()) -> Result<(), ActorError> {
        rt.validate_immediate_caller_accept_any()?;
        log::info!(
            "echo called by {} from origin {}",
            rt.message().caller(),
            rt.message().origin()
        );
        Ok(())
    }

    /// Add a new tag to the state to be signed by the validators
    /// Callable by anyone and designed to be called from Solidity contracts
    pub fn enqueue_tag(rt: &impl Runtime, tag: EnqueueTagParams) -> Result<(), ActorError> {
        rt.validate_immediate_caller_accept_any()?;

        let calling_contract = rt
            .lookup_delegated_address(rt.message().caller().id().unwrap())
            .ok_or(ActorError::assertion_failed(
                "No delegated address for caller".to_string(),
            ))?;
        let calling_eth_address = &calling_contract.payload_bytes()[1..];
        assert!(calling_eth_address.len() == 20, "Invalid eth address length");

        // hash together the calling address and the tag to create a unique identifier for the tag
        let mut hashdata = Vec::new();
        hashdata.extend_from_slice(&calling_eth_address);
        hashdata.extend_from_slice(&tag.tag.0);
        let mut signing_tag = [0x0_u8; 32];
        signing_tag.copy_from_slice(&Keccak256::digest(hashdata));

        log::info!(
            "cetf actor enqueue_tag called by {} with tag {:?}. Resulting signing tag is {:?}",
            hex::encode(calling_eth_address),
            tag,
            &signing_tag,
        );

        rt.transaction(|st: &mut State, rt| {
            if st.enabled {
                let scheduled_epoch = rt.curr_epoch() + 1;
                // NOTE: use of epoch is intentional here. In fendermint the epoch is the block height
                log::info!(
                    "Cetf actor enqueue_tag called by {} with tag {:?} for height: {}",
                    rt.message().caller(),
                    &signing_tag,
                    scheduled_epoch
                );
                st.add_tag_at_height(rt, &(scheduled_epoch as u64), &signing_tag.into())?;
            } else {
                log::info!("CETF actor is disabled. Not all validators have added their keys. No tag was enqueued.");
            }
            Ok(())
        })?;

        Ok(())
    }

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
        rt.validate_immediate_caller_is(std::iter::once(&SYSTEM_ACTOR_ADDR))?;
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
        rt.validate_immediate_caller_is(std::iter::once(&SYSTEM_ACTOR_ADDR))?;
        rt.transaction(|st: &mut State, rt| {
            st.add_signed_blockheight_tag_at_height(rt, &params.height, &params.signature)?;
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
    }
}
