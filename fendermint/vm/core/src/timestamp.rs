use std::time::SystemTime;

// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use serde::{Deserialize, Serialize};

/// Unix timestamp (in seconds).
#[derive(Clone, Debug, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Timestamp(pub u64);

impl Timestamp {
    pub fn as_secs(&self) -> i64 {
        self.0 as i64
    }

    pub fn current() -> Self {
        let d = std::time::SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("duration since epoch");
        Self(d.as_secs())
    }
}
