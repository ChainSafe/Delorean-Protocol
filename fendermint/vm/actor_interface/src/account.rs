// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use fvm_ipld_encoding::tuple::*;
use fvm_shared::address::Address;

define_code!(ACCOUNT { code_id: 4 });

/// State includes the address for the actor
#[derive(Serialize_tuple, Deserialize_tuple, Debug, Clone)]
pub struct State {
    pub address: Address,
}
