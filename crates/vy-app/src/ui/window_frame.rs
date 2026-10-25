//! Client-side window frame handling.
//!
//! `TitlebarOptions::appears_transparent` only affects macOS and Windows. On
//! Linux the compositor is asked for client-side decorations through
//! `WindowOptions::window_decorations`, and the application then owns the
//! frame: it reserves an invisible margin for resize handles with
//! [`gpui::Window::set_client_inset`] and draws its own titlebar inside that
//! margin.
//!
//! Compositors may still refuse client-side decorations (or the window may be
//! tiled/maximized). In that case the platform reports
//! [`Decorations::Server`] and no margin is reserved.

use gpui::{
    CursorStyle, Decorations, Div, MouseButton, Pixels, ResizeEdge, Rgba, Tiling, div, prelude::*,
    px,
};

/// Width of the invisible margin reserved for resize handles on the untiled
/// edges of a client-side decorated window.
pub const CLIENT_INSET: f32 = 4.0;

const ZERO: Pixels = px(0.0);

/// Frame geometry for the current frame.
#[derive(Clone, Copy, Debug)]
pub struct WindowFrame {
    inset: Pixels,
    tiling: Tiling,
}

impl WindowFrame {
    /// Reads the frame configuration reported by the platform.
    pub fn new(decorations: Decorations) -> Self {
        match decorations {
            Decorations::Server => Self {
                inset: ZERO,
                tiling: Tiling::default(),
            },
            Decorations::Client { tiling } => Self {
                inset: px(CLIENT_INSET),
                tiling,
            },
        }
    }

    /// Width of the resize margin. Pass this to
    /// [`gpui::Window::set_client_inset`].
    pub fn inset(&self) -> Pixels {
        self.inset
    }

    /// The titlebar sits flush with the top of the window, so no space is
    /// reserved there; the top resize band overlaps the titlebar instead.
    fn top(&self) -> Pixels {
        ZERO
    }

    fn bottom(&self) -> Pixels {
        if self.tiling.bottom { ZERO } else { self.inset }
    }

    fn left(&self) -> Pixels {
        if self.tiling.left { ZERO } else { self.inset }
    }

    fn right(&self) -> Pixels {
        if self.tiling.right { ZERO } else { self.inset }
    }

    /// Space the frame takes horizontally. Subtract this from the window width
    /// before sizing content.
    pub fn horizontal(&self) -> Pixels {
        self.left() + self.right()
    }

    /// Space the frame takes vertically. Subtract this from the window height
    /// before sizing content.
    pub fn vertical(&self) -> Pixels {
        self.top() + self.bottom()
    }
}

/// Wraps `content` in the window frame.
///
/// The margin is filled with `background`, and invisible resize handles are
/// placed on the untiled edges. `content` is padded so it never overlaps the
/// handles.
pub fn apply(frame: WindowFrame, background: Rgba, content: impl IntoElement) -> impl IntoElement {
    let mut padded = div().flex().flex_col().size_full();
    if frame.top() > ZERO {
        padded = padded.pt(frame.top());
    }
    if frame.bottom() > ZERO {
        padded = padded.pb(frame.bottom());
    }
    if frame.left() > ZERO {
        padded = padded.pl(frame.left());
    }
    if frame.right() > ZERO {
        padded = padded.pr(frame.right());
    }
    let padded = padded.child(content);

    div()
        .id("vy-window-frame")
        .relative()
        .size_full()
        .bg(background)
        .child(padded)
        .children(resize_handles(frame))
}

fn resize_handles(frame: WindowFrame) -> Vec<Div> {
    let inset = frame.inset();
    if inset <= ZERO {
        return Vec::new();
    }

    resize_edges(&frame.tiling)
        .into_iter()
        .map(|edge| place(resize_handle(edge), edge, inset))
        .collect()
}

/// Edges that need a resize handle. Tiled edges are owned by the compositor,
/// and a corner needs both of its edges to be free.
fn resize_edges(tiling: &Tiling) -> Vec<ResizeEdge> {
    let mut edges = Vec::new();
    if !tiling.top {
        edges.push(ResizeEdge::Top);
    }
    if !tiling.bottom {
        edges.push(ResizeEdge::Bottom);
    }
    if !tiling.left {
        edges.push(ResizeEdge::Left);
    }
    if !tiling.right {
        edges.push(ResizeEdge::Right);
    }
    // Corners come last so they win hit testing over the edges.
    if !tiling.top && !tiling.left {
        edges.push(ResizeEdge::TopLeft);
    }
    if !tiling.top && !tiling.right {
        edges.push(ResizeEdge::TopRight);
    }
    if !tiling.bottom && !tiling.left {
        edges.push(ResizeEdge::BottomLeft);
    }
    if !tiling.bottom && !tiling.right {
        edges.push(ResizeEdge::BottomRight);
    }
    edges
}

fn place(handle: Div, edge: ResizeEdge, inset: Pixels) -> Div {
    match edge {
        ResizeEdge::Top => handle.top_0().left_0().right_0().h(inset),
        ResizeEdge::Bottom => handle.bottom_0().left_0().right_0().h(inset),
        ResizeEdge::Left => handle.left_0().top_0().bottom_0().w(inset),
        ResizeEdge::Right => handle.right_0().top_0().bottom_0().w(inset),
        ResizeEdge::TopLeft => handle.top_0().left_0().size(inset),
        ResizeEdge::TopRight => handle.top_0().right_0().size(inset),
        ResizeEdge::BottomLeft => handle.bottom_0().left_0().size(inset),
        ResizeEdge::BottomRight => handle.bottom_0().right_0().size(inset),
    }
}

fn resize_handle(edge: ResizeEdge) -> Div {
    div().absolute().cursor(resize_cursor(edge)).on_mouse_down(
        MouseButton::Left,
        move |_event, window, _cx| {
            window.start_window_resize(edge);
        },
    )
}

fn resize_cursor(edge: ResizeEdge) -> CursorStyle {
    match edge {
        ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
        ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_decorations_reserve_no_margin() {
        let frame = WindowFrame::new(Decorations::Server);
        assert_eq!(frame.inset(), ZERO);
        assert_eq!(frame.horizontal(), ZERO);
        assert_eq!(frame.vertical(), ZERO);
    }

    #[test]
    fn client_decorations_reserve_the_inset_on_the_sides_but_not_on_top() {
        let frame = WindowFrame::new(Decorations::Client {
            tiling: Tiling::default(),
        });
        assert_eq!(frame.inset(), px(CLIENT_INSET));
        assert_eq!(frame.horizontal(), px(CLIENT_INSET * 2.0));
        assert_eq!(frame.vertical(), px(CLIENT_INSET));
    }

    #[test]
    fn tiled_edges_are_not_reserved() {
        let frame = WindowFrame::new(Decorations::Client {
            tiling: Tiling {
                top: true,
                left: true,
                ..Tiling::default()
            },
        });
        assert_eq!(frame.horizontal(), px(CLIENT_INSET));
        assert_eq!(frame.vertical(), px(CLIENT_INSET));
    }

    #[test]
    fn tiled_edges_get_no_resize_handles() {
        assert!(resize_edges(&Tiling::tiled()).is_empty());
        assert_eq!(resize_edges(&Tiling::default()).len(), 8);

        let edges = resize_edges(&Tiling {
            top: true,
            ..Tiling::default()
        });
        assert_eq!(edges.len(), 5);
        assert!(!edges.contains(&ResizeEdge::Top));
        assert!(!edges.contains(&ResizeEdge::TopLeft));
        assert!(!edges.contains(&ResizeEdge::TopRight));
        assert!(edges.contains(&ResizeEdge::BottomLeft));
    }
}
