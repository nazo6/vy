//! Typed host events.
//!
//! A namespace declares an event with `#[vy_api_event]`; the macro generates a
//! marker type implementing [`HostEvent`]. The runtime can then emit the event
//! without passing a topic string and a loosely typed JSON value separately.

use serde::Serialize;

/// One event the host can send to a script.
pub trait HostEvent {
    /// Wire topic, for example `process.onStdout`.
    const TOPIC: &'static str;
    /// Payload sent to the listener.
    type Payload: Serialize;
}

/// A host that can emit typed events.
pub trait EventTransport {
    fn emit_event<E: HostEvent>(&self, payload: &E::Payload);
}
