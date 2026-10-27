//! The surface stack (ADR 0032 Phase 4).
//!
//! A [`Surface`] wraps one [`SurfaceKind`] with the stack metadata ADR
//! 0032 requires: an id visible to `when` expressions as `surfaceId`,
//! a placement, modality, the pane to restore when the stack empties,
//! and its own scroll state. Stack order is the z-order: the last
//! surface is the frontmost and owns input while it is modal.
//!
//! Two fields ADR 0032 names are deliberately not stored yet. `z` is
//! the position in the stack, not a separate number. A per-surface
//! `FocusPath` arrives with plugin surfaces (Phase 6): client overlays
//! have no widget tree, so a stored path would be dead today.

use gpui_base::VirtualListScrollHandle;
use vy_app::overlay::{SurfaceKind, SurfacePlacement};
use vy_core::PaneId;

use super::EditorView;

/// One entry of the surface stack.
pub struct Surface {
    /// Id visible as `surfaceId` in `when` expressions.
    pub(crate) id: String,
    /// What the surface shows.
    pub(crate) kind: SurfaceKind,
    /// Where the surface renders.
    pub(crate) placement: SurfacePlacement,
    /// Whether the surface owns input and blocks the editor behind it.
    /// All client overlays are modal today; non-modal surfaces arrive
    /// with plugin surfaces (Phase 6).
    pub(crate) modal: bool,
    /// Active pane when the surface opened, restored when the stack
    /// becomes empty. [`None`] keeps the current pane.
    pub(crate) restore: Option<PaneId>,
    /// The surface's own list scroll state (replaces the single
    /// `palette_scroll_handle`).
    pub(crate) scroll: VirtualListScrollHandle,
}

impl Surface {
    pub(crate) fn new(id: String, kind: SurfaceKind, restore: Option<PaneId>) -> Self {
        let placement = kind.placement();
        Self {
            id,
            kind,
            placement,
            modal: true,
            restore,
            scroll: VirtualListScrollHandle::new(),
        }
    }
}

/// Answers a discarded Custom picker's pending request with `Null`, so a
/// replaced or cleared picker never leaves its caller waiting. Cancel and
/// confirm reply from the popped surface first, so they must not call this.
fn discard_picker_reply(kind: &mut SurfaceKind) {
    if let SurfaceKind::Palette(palette) = kind
        && let vy_app::palette::PaletteMode::Custom { reply, .. } = &mut palette.mode
        && let Some(request) = reply.take()
    {
        request.respond_value(serde_json::Value::Null);
    }
}

/// Base id for a kind. Kinds appear at most once in the stack today
/// (top-level opens replace the top, drill-ins push a different kind),
/// so the base id is unique unless a caller pushes a duplicate.
fn base_id(kind: &SurfaceKind) -> &'static str {
    match kind {
        SurfaceKind::Palette(palette) => match &palette.mode {
            vy_app::palette::PaletteMode::Custom { .. } => "picker",
            _ => "palette",
        },
        SurfaceKind::Ex(_) => "ex",
        SurfaceKind::Prompt(_) => "prompt",
        SurfaceKind::Approval(_) => "approval",
        SurfaceKind::Settings(_) => "settings",
        SurfaceKind::SettingChoice(_) => "setting-choice",
        SurfaceKind::SettingPrompt(_) => "setting-prompt",
    }
}

impl EditorView {
    /// The frontmost surface, if any.
    pub(crate) fn surface_top(&self) -> Option<&Surface> {
        self.surfaces.last()
    }

    /// The frontmost surface, mutably.
    pub(crate) fn surface_top_mut(&mut self) -> Option<&mut Surface> {
        self.surfaces.last_mut()
    }

    /// The frontmost surface's kind, if any.
    pub(crate) fn surface_top_kind(&self) -> Option<&SurfaceKind> {
        self.surfaces.last().map(|surface| &surface.kind)
    }

    /// Pushes a surface on top of the stack (drill-in: prompt, choice).
    pub(crate) fn push_surface(&mut self, kind: SurfaceKind) {
        let base = base_id(&kind);
        let id = if self.surfaces.iter().any(|surface| surface.id == base) {
            self.next_surface_id += 1;
            format!("{base}-{}", self.next_surface_id)
        } else {
            base.to_string()
        };
        let restore = self.active_pane;
        self.surfaces.push(Surface::new(id, kind, restore));
    }

    /// Pops the frontmost surface. When the stack becomes empty, the
    /// active pane returns to the pane the bottom surface restored.
    pub(crate) fn pop_surface(&mut self) -> Option<Surface> {
        let popped = self.surfaces.pop()?;
        if self.surfaces.is_empty()
            && let Some(pane) = popped.restore
        {
            self.active_pane = Some(pane);
        }
        Some(popped)
    }

    /// Replaces the frontmost surface (top-level opens: palette, Ex,
    /// settings, picker). An empty stack just pushes. A replaced Custom
    /// picker is answered with `Null` first (ADR 0032 review M2).
    pub(crate) fn replace_top_surface(&mut self, kind: SurfaceKind) {
        if self.surfaces.is_empty() {
            self.push_surface(kind);
        } else {
            if let Some(top) = self.surfaces.last_mut() {
                discard_picker_reply(&mut top.kind);
            }
            let restore = self.surfaces.last().and_then(|surface| surface.restore);
            let base = base_id(&kind);
            let id = if self
                .surfaces
                .iter()
                .take(self.surfaces.len().saturating_sub(1))
                .any(|surface| surface.id == base)
            {
                self.next_surface_id += 1;
                format!("{base}-{}", self.next_surface_id)
            } else {
                base.to_string()
            };
            let top = self.surfaces.last_mut().expect("non-empty stack");
            *top = Surface::new(id, kind, restore);
        }
    }

    /// Empties the stack, restoring the bottom surface's pane. Any
    /// discarded Custom picker is answered with `Null` first
    /// (ADR 0032 review M2).
    pub(crate) fn clear_surfaces(&mut self) {
        let restore = self.surfaces.first().and_then(|surface| surface.restore);
        for surface in &mut self.surfaces {
            discard_picker_reply(&mut surface.kind);
        }
        self.surfaces.clear();
        if let Some(pane) = restore {
            self.active_pane = Some(pane);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vy_app::palette::{PaletteMode, PaletteState};

    fn custom_picker(reply: Option<vy_js::HostRequest>) -> SurfaceKind {
        SurfaceKind::Palette(PaletteState::with_mode(PaletteMode::Custom {
            id: "test".to_string(),
            items: vec![],
            placeholder: None,
            reply,
        }))
    }

    #[test]
    fn discard_picker_reply_answers_pending_request_with_null() {
        let (request, receiver) =
            vy_js::HostRequest::new("ui.picker.open", serde_json::Value::Null);
        let mut kind = custom_picker(Some(request));
        discard_picker_reply(&mut kind);
        assert!(matches!(
            receiver.try_recv(),
            Ok(vy_js::HostReply::Ok(serde_json::Value::Null))
        ));
        // The reply is taken: a second discard answers nothing.
        discard_picker_reply(&mut kind);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn discard_picker_reply_ignores_missing_reply_and_other_kinds() {
        let mut kind = custom_picker(None);
        discard_picker_reply(&mut kind);
        let mut kind = SurfaceKind::Ex(vy_app::ex::ExState::default());
        discard_picker_reply(&mut kind);
    }
}
