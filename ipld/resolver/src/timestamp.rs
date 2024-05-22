// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use serde::{Deserialize, Serialize};
use std::ops::{Add, Sub};
use std::time::{Duration, SystemTime};

/// Unix timestamp in seconds since epoch, which we can use to select the
/// more recent message during gossiping.
#[derive(Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Debug, Serialize, Deserialize, Default)]
pub struct Timestamp(u64);

impl Timestamp {
    /// Current timestamp.
    pub fn now() -> Self {
        let secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("now() is never before UNIX_EPOCH")
            .as_secs();
        Self(secs)
    }

    /// Seconds elapsed since Unix epoch.
    pub fn as_secs(&self) -> u64 {
        self.0
    }
}

impl Sub<Duration> for Timestamp {
    type Output = Self;

    fn sub(self, rhs: Duration) -> Self {
        Self(self.as_secs().saturating_sub(rhs.as_secs()))
    }
}

impl Add<Duration> for Timestamp {
    type Output = Self;

    fn add(self, rhs: Duration) -> Self {
        Self(self.as_secs().saturating_add(rhs.as_secs()))
    }
}

#[cfg(any(test, feature = "arb"))]
mod arb {
    use super::Timestamp;

    impl quickcheck::Arbitrary for Timestamp {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            Self(u64::arbitrary(g).saturating_add(1))
        }
    }
}
