// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use fvm_ipld_encoding::tuple::*;
use fvm_shared::address::Address;
use fvm_shared::MethodNum;
use fvm_shared::METHOD_CONSTRUCTOR;

define_singleton!(CRON { id: 3, code_id: 3 });

/// Cron actor methods available.
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    EpochTick = 2,
}

/// Cron actor state which holds entries to call during epoch tick
#[derive(Default, Serialize_tuple, Deserialize_tuple, Clone, Debug)]
pub struct State {
    /// Entries is a set of actors (and corresponding methods) to call during EpochTick.
    pub entries: Vec<Entry>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct Entry {
    /// The actor to call (ID address)
    pub receiver: Address,
    /// The method number to call (must accept empty parameters)
    pub method_num: MethodNum,
}
