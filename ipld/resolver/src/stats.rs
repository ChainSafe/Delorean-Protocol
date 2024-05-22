// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: MIT
use lazy_static::lazy_static;
use prometheus::{Histogram, HistogramOpts, IntCounter, IntGauge, Registry};

macro_rules! metrics {
    ($($name:ident : $type:ty = $make:expr);* $(;)?) => {
        $(
          lazy_static! {
            pub static ref $name: $type = $make.unwrap();
          }
        )*

        pub fn register_metrics(registry: &Registry) -> anyhow::Result<()> {
          $(registry.register(Box::new($name.clone()))?;)*
          Ok(())
        }
    };
}

metrics! {
    PING_RTT: Histogram =
        Histogram::with_opts(HistogramOpts::new("ping_rtt", "Ping roundtrip time"));

    PING_TIMEOUT: IntCounter =
        IntCounter::new("ping_timeouts", "Number of timed out pings");

    PING_FAILURE: IntCounter =
        IntCounter::new("ping_failure", "Number of failed pings");

    PING_SUCCESS: IntCounter =
        IntCounter::new("ping_success", "Number of successful pings",);

    IDENTIFY_FAILURE: IntCounter =
        IntCounter::new("identify_failure", "Number of Identify errors",);

    IDENTIFY_RECEIVED: IntCounter =
        IntCounter::new("identify_received", "Number of Identify infos received",);

    DISCOVERY_BACKGROUND_LOOKUP: IntCounter = IntCounter::new(
        "discovery_background_lookup",
        "Number of background lookups started",
    );

    DISCOVERY_CONNECTED_PEERS: IntGauge =
        IntGauge::new("discovery_connected_peers", "Number of connections",);

    MEMBERSHIP_SKIPPED_PEERS: IntCounter =
        IntCounter::new("membership_skipped_peers", "Number of providers skipped",);

    MEMBERSHIP_ROUTABLE_PEERS: IntGauge =
        IntGauge::new("membership_routable_peers", "Number of routable peers");

    MEMBERSHIP_PROVIDER_PEERS: IntGauge =
        IntGauge::new("membership_provider_peers", "Number of unique providers");

    MEMBERSHIP_UNKNOWN_TOPIC: IntCounter = IntCounter::new(
        "membership_unknown_topic",
        "Number of messages with unknown topic"
    );

    MEMBERSHIP_INVALID_MESSAGE: IntCounter = IntCounter::new(
        "membership_invalid_message",
        "Number of invalid messages received"
    );

    MEMBERSHIP_PUBLISH_SUCCESS: IntCounter = IntCounter::new(
      "membership_publish_total", "Number of published messages"
    );

    MEMBERSHIP_PUBLISH_FAILURE: IntCounter = IntCounter::new(
        "membership_publish_failure",
        "Number of failed publish attempts"
    );

    CONTENT_RESOLVE_RUNNING: IntGauge = IntGauge::new(
        "content_resolve_running",
        "Number of currently running content resolutions"
    );

    CONTENT_RESOLVE_NO_PEERS: IntCounter = IntCounter::new(
        "content_resolve_no_peers",
        "Number of resolutions with no known peers"
    );

    CONTENT_RESOLVE_SUCCESS: IntCounter = IntCounter::new(
        "content_resolve_success",
        "Number of successful resolutions"
    );

    CONTENT_RESOLVE_FAILURE: IntCounter = IntCounter::new(
      "content_resolve_failure",
      "Number of failed resolutions"
    );

    CONTENT_RESOLVE_FALLBACK: IntCounter = IntCounter::new(
        "content_resolve_fallback",
        "Number of resolutions that fall back on secondary peers"
    );

    CONTENT_RESOLVE_PEERS: Histogram = Histogram::with_opts(HistogramOpts::new(
        "content_resolve_peers",
        "Number of peers found for resolution from a subnet"
    ));

    CONTENT_CONNECTED_PEERS: Histogram = Histogram::with_opts(HistogramOpts::new(
        "content_connected_peers",
        "Number of connected peers in a resolution"
    ));

    CONTENT_RATE_LIMITED: IntCounter = IntCounter::new(
        "content_rate_limited",
        "Number of rate limited requests"
    );
}
