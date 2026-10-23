//! Statusline slots for user-controlled UI.
//!
//! Scripts register items via `ui.statusline.createItem(options)`. Items are
//! arranged into left, center, and right groups and sorted by priority (higher
//! priority placed towards the outer edges). See ADR 0006.

use std::collections::HashMap;

/// Horizontal alignment of a statusline item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::IntoStaticStr, strum::EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum StatuslineAlignment {
    Left,
    Center,
    Right,
}

impl StatuslineAlignment {
    pub fn parse(value: &str) -> Option<Self> {
        value.parse().ok()
    }
}

/// One item displayed in the statusline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatuslineItem {
    pub id: String,
    pub alignment: StatuslineAlignment,
    pub priority: i32,
    pub text: String,
    pub color: Option<String>,
    pub command: Option<String>,
    pub visible: bool,
}

impl StatuslineItem {
    pub fn new(
        id: impl Into<String>,
        alignment: StatuslineAlignment,
        priority: i32,
        text: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            alignment,
            priority,
            text: text.into(),
            color: None,
            command: None,
            visible: true,
        }
    }
}

/// Collection of all registered statusline items.
#[derive(Debug, Default, Clone)]
pub struct Statusline {
    items: HashMap<String, StatuslineItem>,
}

impl Statusline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_item(&mut self, item: StatuslineItem) {
        self.items.insert(item.id.clone(), item);
    }

    pub fn update_item(
        &mut self,
        id: &str,
        text: Option<String>,
        color: Option<Option<String>>,
        command: Option<Option<String>>,
        visible: Option<bool>,
    ) -> bool {
        let Some(item) = self.items.get_mut(id) else {
            return false;
        };

        if let Some(text) = text {
            item.text = text;
        }
        if let Some(color) = color {
            item.color = color;
        }
        if let Some(command) = command {
            item.command = command;
        }
        if let Some(visible) = visible {
            item.visible = visible;
        }
        true
    }

    pub fn dispose_item(&mut self, id: &str) -> bool {
        self.items.remove(id).is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.items.values().all(|item| !item.visible)
    }

    /// Returns visible items for the given alignment, sorted by priority descending.
    pub fn items_for(&self, alignment: StatuslineAlignment) -> Vec<&StatuslineItem> {
        let mut matching: Vec<&StatuslineItem> = self
            .items
            .values()
            .filter(|item| item.visible && item.alignment == alignment)
            .collect();

        // Higher priority items are placed further to the outside:
        // - Left: high priority first (left-most)
        // - Right: high priority last or first (conventionally high priority is right-most)
        // We sort by priority descending.
        matching.sort_by_key(|a| std::cmp::Reverse(a.priority));
        matching
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_updates_and_disposes_items() {
        let mut statusline = Statusline::new();
        assert!(statusline.is_empty());

        statusline.set_item(StatuslineItem::new(
            "item1",
            StatuslineAlignment::Left,
            10,
            "hello",
        ));
        assert!(!statusline.is_empty());

        let left = statusline.items_for(StatuslineAlignment::Left);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].text, "hello");

        statusline.update_item("item1", Some("world".into()), None, None, None);
        let left = statusline.items_for(StatuslineAlignment::Left);
        assert_eq!(left[0].text, "world");

        statusline.update_item("item1", None, None, None, Some(false));
        assert!(statusline.is_empty());

        statusline.dispose_item("item1");
        assert_eq!(statusline.items.len(), 0);
    }

    #[test]
    fn sorts_by_priority_descending() {
        let mut statusline = Statusline::new();
        statusline.set_item(StatuslineItem::new(
            "low",
            StatuslineAlignment::Left,
            0,
            "low",
        ));
        statusline.set_item(StatuslineItem::new(
            "high",
            StatuslineAlignment::Left,
            100,
            "high",
        ));
        statusline.set_item(StatuslineItem::new(
            "mid",
            StatuslineAlignment::Left,
            50,
            "mid",
        ));

        let left = statusline.items_for(StatuslineAlignment::Left);
        let ids: Vec<&str> = left.into_iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, vec!["high", "mid", "low"]);
    }
}
