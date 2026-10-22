//! Focus stack management for modal overlays and editor components.

use gpui::{App, FocusHandle, Window};

/// Manages a stack of focus handles for modal dialogs and overlays.
#[derive(Debug, Default)]
pub struct FocusStack {
    stack: Vec<FocusHandle>,
}

impl FocusStack {
    pub fn new() -> Self {
        Self::default()
    }

    /// Pushes the previous focus handle onto the stack and shifts window focus to `next`.
    pub fn push(
        &mut self,
        previous: FocusHandle,
        next: &FocusHandle,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.stack.push(previous);
        next.focus(window, cx);
    }

    /// Pops the previous focus handle and restores window focus to it.
    pub fn pop(&mut self, window: &mut Window, cx: &mut App) -> Option<FocusHandle> {
        let previous = self.stack.pop()?;
        previous.focus(window, cx);
        Some(previous)
    }

    /// Returns the number of focus handles currently in the stack.
    pub fn len(&self) -> usize {
        self.stack.len()
    }

    /// Returns whether any focus handle is currently stored on the stack.
    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Clears the focus stack without restoring window focus.
    pub fn clear(&mut self) {
        self.stack.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_stack_lifecycle() {
        let mut stack = FocusStack::new();
        assert!(stack.is_empty());
        assert_eq!(stack.len(), 0);
        stack.clear();
        assert!(stack.is_empty());
    }
}
