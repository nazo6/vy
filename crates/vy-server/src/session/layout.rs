use vy_proto::{
    LayoutDidChangeNotification, LayoutGetResult, LayoutPaneInfo, RpcMessage, to_value,
};

use super::{Outcome, Session};

impl Session {
    /// Returns the whole layout. The client uses this after reconnecting.
    pub(super) fn layout_get(&self, id: u64) -> Outcome {
        let panes = self
            .layout
            .leaves()
            .into_iter()
            .filter_map(|pane_id| {
                self.panes.get(&pane_id).map(|pane| LayoutPaneInfo {
                    pane_id,
                    view: pane.view.clone(),
                    document_id: pane.document_id,
                    uri: pane
                        .document_id
                        .and_then(|document_id| self.documents.get(&document_id))
                        .map(|buffer| buffer.uri().as_str().to_string()),
                })
            })
            .collect();

        let result = LayoutGetResult {
            window_id: Some(self.layout.window_id()),
            revision: self.layout.revision(),
            layout: self.layout.root().cloned(),
            floating: Vec::new(),
            active_pane: self.layout.active_pane(),
            panes,
        };

        Outcome::one(RpcMessage::ok(id, to_value(&result)))
    }

    pub(super) fn layout_notification(&self, reason: &str) -> RpcMessage {
        RpcMessage::notification(
            "layout/did_change",
            to_value(&LayoutDidChangeNotification {
                window_id: Some(self.layout.window_id()),
                revision: self.layout.revision(),
                layout: self.layout.root().cloned(),
                floating: Vec::new(),
                active_pane: self.layout.active_pane(),
                reason: Some(reason.to_string()),
            }),
        )
    }
}
