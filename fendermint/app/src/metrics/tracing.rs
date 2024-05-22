// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
//! Subscribing to tracing events and turning them into metrics.

use std::marker::PhantomData;

use tracing::{Event, Subscriber};
use tracing_subscriber::{filter, layer, registry::LookupSpan, Layer};

use super::prometheus::app as am;
use crate::events::*;

/// Create a layer that handles events by incrementing metrics.
pub fn layer<S>() -> impl Layer<S>
where
    S: Subscriber,
    for<'a> S: LookupSpan<'a>,
{
    MetricsLayer::new().with_filter(filter::filter_fn(|md| md.name().starts_with("event::")))
}

struct MetricsLayer<S> {
    _subscriber: PhantomData<S>,
}

impl<S> MetricsLayer<S> {
    pub fn new() -> Self {
        Self {
            _subscriber: PhantomData,
        }
    }
}

/// Check that the field exist on a type; if it doesn't this won't compile.
/// This ensures that we're mapping fields with the correct name.
macro_rules! check_field {
    ($event_ty:ident :: $field:ident) => {{
        if false {
            #[allow(clippy::needless_update)]
            let _event = $event_ty {
                $field: Default::default(),
                ..Default::default()
            };
        }
    }};
}

/// Set a gague to an absolute value based on a field in an event.
macro_rules! set_gauge {
    ($event:ident, $event_ty:ident :: $field:ident, $gauge:expr) => {
        check_field!($event_ty::$field);
        let mut fld = visitors::FindU64::new(stringify!($field));
        $event.record(&mut fld);
        $gauge.set(fld.value as i64);
    };
}

/// Set a gauge to the maximum of its value and a field in an event.
macro_rules! max_gauge {
    ($event:ident, $event_ty:ident :: $field:ident, $gauge:expr) => {
        check_field!($event_ty::$field);
        let mut fld = visitors::FindU64::new(stringify!($field));
        $event.record(&mut fld);
        let curr = $gauge.get();
        $gauge.set(std::cmp::max(fld.value as i64, curr));
    };
}

/// Increment a counter by the value of a field in the event.
macro_rules! inc_counter {
    ($event:ident, $event_ty:ident :: $field:ident, $counter:expr) => {
        check_field!($event_ty::$field);
        let mut fld = visitors::FindU64::new(stringify!($field));
        $event.record(&mut fld);
        $counter.inc_by(fld.value);
    };
}

/// Increment a counter by 1.
///
/// The field is ignored, it's only here because of how the macros look like.
macro_rules! inc1_counter {
    ($event:ident, $event_ty:ident :: $field:ident, $counter:expr) => {
        check_field!($event_ty::$field);
        $counter.inc();
    };
}

/// Produce the prefixed event name from the type name.
macro_rules! event_name {
    ($event_ty:ident) => {
        concat!("event::", stringify!($event_ty))
    };
}

/// Call one of the macros that set values on a metric.
macro_rules! event_mapping {
    ($op:ident, $event:ident, $event_ty:ident :: $field:ident, $metric:expr) => {
        $op!($event, $event_ty::$field, $metric);
    };
}

/// Match the event name to event DTO types and within the map fields to metrics.
macro_rules! event_match {
    ($event:ident { $( $event_ty:ident { $( $field:ident => $op:ident ! $metric:expr  ),* $(,)? } ),* } ) => {
        match $event.metadata().name() {
            $(
                event_name!($event_ty) => {
                    $(
                        event_mapping!($op, $event, $event_ty :: $field, $metric);
                    )*
                }
            )*
            _ => {}
        }
    };
}

impl<S: Subscriber> Layer<S> for MetricsLayer<S> {
    fn on_event(&self, event: &Event<'_>, _ctx: layer::Context<'_, S>) {
        event_match!(event {
            NewParentView {
                block_height              => set_gauge   ! &am::TOPDOWN_VIEW_BLOCK_HEIGHT,
                num_msgs                  => inc_counter ! &am::TOPDOWN_VIEW_NUM_MSGS,
                num_validator_changes     => inc_counter ! &am::TOPDOWN_VIEW_NUM_VAL_CHNGS,
            },
            ParentFinalityCommitted {
                block_height              => set_gauge   ! &am::TOPDOWN_FINALIZED_BLOCK_HEIGHT,
            },
            ParentFinalityVoteAdded {
                // This one can move up and down randomly as votes come in, but statistically should
                // be less likely to be affected by Byzantine validators casting nonsense votes.
                block_height              => set_gauge    ! &am::TOPDOWN_FINALITY_VOTE_BLOCK_HEIGHT,
                // This one should only move up, showing the highest vote in the tally.
                // It should be easy to produce this on Grafana as well from the one above.
                block_height              => max_gauge    ! &am::TOPDOWN_FINALITY_VOTE_MAX_BLOCK_HEIGHT,
                validator                 => inc1_counter ! &am::TOPDOWN_FINALITY_VOTE_ADDED,
            },
            ParentFinalityVoteIgnored {
                validator                 => inc1_counter ! &am::TOPDOWN_FINALITY_VOTE_IGNORED,
            },
            ParentFinalityMissingQuorum {
                block_hash                => inc1_counter ! &am::TOPDOWN_FINALITY_MISSING_QUORUM,
            },
            NewBottomUpCheckpoint {
                block_height              => set_gauge   ! &am::BOTTOMUP_CKPT_BLOCK_HEIGHT,
                next_configuration_number => set_gauge   ! &am::BOTTOMUP_CKPT_CONFIG_NUM,
                num_msgs                  => inc_counter ! &am::BOTTOMUP_CKPT_NUM_MSGS,
            },
            NewBlock {
                block_height              => set_gauge   ! &am::ABCI_COMMITTED_BLOCK_HEIGHT
            }
        });
    }
}

mod visitors {
    use tracing::field::{Field, Visit};

    pub struct FindU64<'a> {
        pub name: &'a str,
        pub value: u64,
    }

    impl<'a> FindU64<'a> {
        pub fn new(name: &'a str) -> Self {
            Self { name, value: 0 }
        }
    }

    // Looking for multiple values because the callsite might be passed as a literal which turns into an i64 for example.
    impl<'a> Visit for FindU64<'a> {
        fn record_u64(&mut self, field: &Field, value: u64) {
            if field.name() == self.name {
                self.value = value;
            }
        }

        fn record_i64(&mut self, field: &Field, value: i64) {
            if field.name() == self.name {
                self.value = value as u64;
            }
        }

        fn record_i128(&mut self, field: &Field, value: i128) {
            if field.name() == self.name {
                self.value = value as u64;
            }
        }

        fn record_u128(&mut self, field: &Field, value: u128) {
            if field.name() == self.name {
                self.value = value as u64;
            }
        }

        fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}
    }
}

#[cfg(test)]
mod tests {
    use fendermint_tracing::emit;
    use fendermint_vm_event::ParentFinalityCommitted;
    use prometheus::IntGauge;
    use tracing_subscriber::layer::SubscriberExt;

    #[test]
    fn test_metrics_layer() {
        let gauge: &IntGauge = &super::super::prometheus::app::TOPDOWN_FINALIZED_BLOCK_HEIGHT;

        let v0 = gauge.get();
        gauge.inc();
        let v1 = gauge.get();
        assert!(v1 > v0, "gague should change without being registered");

        let block_height = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let subscriber = tracing_subscriber::registry().with(super::layer());

        tracing::subscriber::with_default(subscriber, || {
            emit! {
                ParentFinalityCommitted { block_height, block_hash: "metrics-test-block" }
            }
        });

        assert_eq!(
            gauge.get() as u64,
            block_height,
            "metrics should be captured"
        );
    }
}
