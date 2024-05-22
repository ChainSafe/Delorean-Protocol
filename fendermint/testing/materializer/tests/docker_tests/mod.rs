// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
//! These test modules are all imported by the top level `docker.rs` module,
//! so that they can be annotated with the `#[serial]` macro and run one by one,
//! sharing their materializer state.

// Tests using the manifest bearing their name.
pub mod layer2;
pub mod root_only;
pub mod standalone;
