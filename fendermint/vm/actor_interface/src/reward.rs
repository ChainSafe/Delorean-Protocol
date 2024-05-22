// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

// The reward actor is a singleton, but for now let's just use a
// simple account, instead of the one in the built-in actors library,
// because that has too many Filecoin mainnet specific things.

define_id!(REWARD { id: 2 });
