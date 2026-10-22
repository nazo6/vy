use rmpv::Value;
use vy_proto::RpcMessage;
use vy_proto::value::object;

use super::{Outcome, Session, to_value};

impl Session {
    /// Returns the whole layout. The client uses this after reconnecting.
    pub(super) fn layout_get(&self, id: u64) -> Outcome {
        let mut panes = Vec::new();
        for pane_id in self.layout.leaves() {
            if let Some(pane) = self.panes.get(&pane_id) {
                panes.push(object([
                    ("pane_id", Value::from(pane_id)),
                    ("view", Value::from(pane.view.clone())),
                    (
                        "document_id",
                        pane.document_id.map(Value::from).unwrap_or(Value::Nil),
                    ),
                ]));
            }
        }

        let mut value = self.layout_value();
        if let Value::Map(entries) = &mut value {
            entries.push((Value::from("panes"), Value::Array(panes)));
        }

        Outcome::one(RpcMessage::ok(id, value))
    }

    pub(super) fn layout_notification(&self, reason: &str) -> RpcMessage {
        let mut value = self.layout_value();
        if let Value::Map(entries) = &mut value {
            entries.push((Value::from("reason"), Value::from(reason)));
        }
        RpcMessage::notification("layout/did_change", value)
    }

    pub(super) fn layout_value(&self) -> Value {
        object([
            ("window_id", Value::from(self.layout.window_id())),
            ("revision", Value::from(self.layout.revision())),
            (
                "layout",
                self.layout.root().map(to_value).unwrap_or(Value::Nil),
            ),
            ("floating", Value::Array(Vec::new())),
            (
                "active_pane",
                self.layout
                    .active_pane()
                    .map(Value::from)
                    .unwrap_or(Value::Nil),
            ),
        ])
    }
}
