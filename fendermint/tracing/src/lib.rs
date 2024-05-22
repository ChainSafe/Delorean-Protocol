// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

/// Emit an event that conforms to a flat event structure type using the [tracing::event!](https://github.com/tokio-rs/tracing/blob/908cc432a5994f6e17c8f36e13c217dc40085704/tracing/src/macros.rs#L854) macro.
///
/// There should be a [Subnscriber](https://docs.rs/tracing/latest/tracing/trait.Subscriber.html) in the application root to check the
/// [Metadata::name](https://docs.rs/tracing/latest/tracing/struct.Metadata.html#method.name) of the event in the
/// [Event::metadata](https://docs.rs/tracing/latest/tracing/struct.Event.html#method.metadata).
///
/// Once the [valuable](https://github.com/tokio-rs/tracing/discussions/1906) feature is stable,
/// we won't have the restriction of flat events.
///
/// The emitted [tracing::Event] will contain the name of the event twice:
/// in the [tracing::metadata::Metadata::name] field as `"event::<name>"` and under the `event` key in the [tracing::field::ValueSet].
/// The rationale is that we can write a [tracing::Subscriber] that looks for the events it is interested in using
/// the `name`, or find all events by filtering on the `event::` prefix.
/// By default `name` would be `event <file>:<line>`, but it turns out it's impossible to ask the
/// [log formatter](https://github.com/tokio-rs/tracing/blob/908cc432a5994f6e17c8f36e13c217dc40085704/tracing-subscriber/src/fmt/format/mod.rs#L930)
/// to output the `name``, and for all other traces it would be redundant with the filename and line we print,
/// which are available separately on the metadata, hence the `event` key which will be displayed instead.
///
/// ### Example
///
/// ```ignore
/// pub struct NewBottomUpCheckpoint<'a> {
///     pub block_height: u64,
///     pub block_hash: &'a str,
/// }
///
/// let block_height = todo!();
/// let block_hash_hex = hex::encode(todo!());
///
/// emit!(NewBottomUpCheckpoint {
///     block_height,
///     block_hash: &block_hash_hex,
/// });
/// ```
#[macro_export]
macro_rules! emit {
    ($lvl:ident, $event:ident { $($field:ident $(: $value:expr)?),* $(,)? } ) => {{
        // Make sure the emitted fields match the schema of the event.
        if false {
            let _event = $event {
                $($field $(: $value)?),*
            };
        }
        tracing::event!(
            name: concat!("event::", stringify!($event)),
            tracing::Level::$lvl,
            { event = tracing::field::display(stringify!($event)), $($field $(= $value)?),* }
        )
    }};

    ($event:ident { $($field:ident $(: $value:expr)?),* $(,)? } ) => {{
        emit!(INFO, $event { $($field $(: $value)? ),* })
    }};
}

#[cfg(test)]
mod tests {

    #[allow(dead_code)]
    struct TestEvent<'a> {
        pub foo: u32,
        pub bar: &'a str,
    }

    #[test]
    fn test_emit() {
        emit!(TestEvent {
            foo: 123,
            bar: "spam",
        });
    }
}
