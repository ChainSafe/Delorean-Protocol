// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
//! Placeholders can be used for delegated address types.
//! The FVM automatically creates one if the recipient of a transaction
//! doesn't exist. Then, the executor replaces the code later based on
//! the namespace in the delegated address.

define_code!(PLACEHOLDER { code_id: 13 });
