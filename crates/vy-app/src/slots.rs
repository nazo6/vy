//! Gutter and Hover slots for user-controlled editor UI.
//!
//! Scripts register gutter providers or items via `ui.gutter`, and hover
//! providers or floating popups via `ui.hover`.

use std::collections::HashMap;

use crate::color::ThemeColor;

/// An item or mark displayed in the gutter next to a line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GutterItem {
    pub id: String,
    pub line: usize,
    pub text: String,
    pub color: Option<ThemeColor>,
}

impl GutterItem {
    pub fn new(id: impl Into<String>, line: usize, text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            line,
            text: text.into(),
            color: None,
        }
    }
}

/// Registry of gutter items across buffers/lines.
#[derive(Debug, Default, Clone)]
pub struct GutterRegistry {
    items: HashMap<String, GutterItem>,
}

impl GutterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_item(&mut self, item: GutterItem) {
        self.items.insert(item.id.clone(), item);
    }

    pub fn dispose_item(&mut self, id: &str) -> bool {
        self.items.remove(id).is_some()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Returns all gutter items for a specific line number.
    pub fn items_for_line(&self, line: usize) -> Vec<&GutterItem> {
        self.items
            .values()
            .filter(|item| item.line == line)
            .collect()
    }

    /// Whether there are any registered items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// A hover popup displayed over a buffer location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverState {
    pub line: usize,
    pub character: usize,
    pub text: String,
}

impl HoverState {
    pub fn new(line: usize, character: usize, text: impl Into<String>) -> Self {
        Self {
            line,
            character,
            text: text.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gutter_registry_adds_queries_and_removes() {
        let mut registry = GutterRegistry::new();
        assert!(registry.is_empty());

        registry.set_item(GutterItem::new("git1", 10, "+"));
        registry.set_item(GutterItem::new("diag1", 10, "E"));
        registry.set_item(GutterItem::new("bp1", 20, "●"));

        assert_eq!(registry.items_for_line(10).len(), 2);
        assert_eq!(registry.items_for_line(20).len(), 1);
        assert_eq!(registry.items_for_line(5).len(), 0);

        registry.dispose_item("git1");
        assert_eq!(registry.items_for_line(10).len(), 1);
        assert_eq!(registry.items_for_line(10)[0].id, "diag1");
    }
}
