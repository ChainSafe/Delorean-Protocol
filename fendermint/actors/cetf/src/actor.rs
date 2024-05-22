// Copyright 2021-2023 BadBoi Labs
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_actors_runtime::actor_dispatch;
use fil_actors_runtime::actor_error;
use fil_actors_runtime::builtin::singletons::SYSTEM_ACTOR_ADDR;
use fil_actors_runtime::runtime::{ActorCode, Runtime};
use fil_actors_runtime::ActorError;

use crate::EnqueueTagParams;
use crate::{Method, CETF_ACTOR_NAME};

fil_actors_runtime::wasm_trampoline!(Actor);

fvm_sdk::sys::fvm_syscalls! {
    module = "cetf_kernel";
    pub fn enqueue_tag(tag: *const u8, tag_len: u32) -> Result<()>;
}

pub struct Actor;
impl Actor {
    fn enqueue_tag(rt: &impl Runtime, params: EnqueueTagParams) -> Result<(), ActorError> {
        rt.validate_immediate_caller_is(std::iter::once(&SYSTEM_ACTOR_ADDR))?;
        unsafe {
            enqueue_tag(params.tag.as_ptr(), params.tag.len().try_into().unwrap()).unwrap();
            Ok(())
        }
    }
}

impl ActorCode for Actor {
    type Methods = Method;

    fn name() -> &'static str {
        CETF_ACTOR_NAME
    }

    actor_dispatch! {
        EnqueueTag => enqueue_tag,
    }
}
