// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use std::time::{Duration, Instant};

use gcra::GcraState;
pub use gcra::RateLimit;
use lru_time_cache::LruCache;

/// Track the rate limit of resources (e.g. bytes) consumed per key.
///
/// Forgets keys after long periods of inactivity.
pub struct RateLimiter<K> {
    cache: LruCache<K, GcraState>,
}

impl<K> RateLimiter<K>
where
    K: Ord + Clone,
{
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: LruCache::with_expiry_duration(ttl),
        }
    }

    /// Try to add a certain amount of resources consumed to a key.
    ///
    /// Return `true` if the key was within limits, `false` if it needs to wait.
    ///
    /// The [`RateLimit`] is passed in so that we can update it dynamically
    /// based on how much data we anticipate we will have to serve.
    pub fn add(&mut self, limit: &RateLimit, key: K, cost: u32) -> bool {
        self.add_at(limit, key, cost, Instant::now())
    }

    /// Same as [`RateLimiter::add`] but allows passing in the time, for testing.
    pub fn add_at(&mut self, limit: &RateLimit, key: K, cost: u32, at: Instant) -> bool {
        #[allow(clippy::unwrap_or_default)]
        let state = self.cache.entry(key).or_insert_with(GcraState::default);

        state.check_and_modify_at(limit, at, cost).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{RateLimit, RateLimiter};

    #[test]
    fn basics() {
        // 10Mb per hour.
        let one_hour = Duration::from_secs(60 * 60);
        let rate_limit = RateLimit::new(10 * 1024 * 1024, one_hour);
        let mut rate_limiter = RateLimiter::<&'static str>::new(one_hour);

        assert!(rate_limiter.add(&rate_limit, "foo", 1024));
        assert!(rate_limiter.add(&rate_limit, "foo", 5 * 1024 * 1024));
        assert!(
            !rate_limiter.add(&rate_limit, "foo", 5 * 1024 * 1024),
            "can't over consume"
        );
        assert!(
            rate_limiter.add(&rate_limit, "bar", 5 * 1024 * 1024),
            "others can consume"
        );

        assert!(
            rate_limiter.add_at(
                &rate_limit,
                "foo",
                5 * 1024 * 1024,
                Instant::now() + one_hour + Duration::from_secs(1)
            ),
            "can consume again in the future"
        );

        let rate_limit = RateLimit::new(50 * 1024 * 1024, one_hour);
        assert!(
            rate_limiter.add(&rate_limit, "bar", 15 * 1024 * 1024),
            "can raise quota"
        );
    }
}
