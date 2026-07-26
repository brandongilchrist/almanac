//! Minimal Nostr-event-shaped source type.
//!
//! Almanac ingests Nostr events but does not depend on a full Nostr SDK —
//! the only fields it reads are `kind`, `content`, `tags`, and `created_at`.
//! This struct mirrors that surface so ingestors are pure functions over
//! a small, testable shape. Production code adapts real Nostr events into
//! [`SourceEvent`] at the boundary.

use serde::{Deserialize, Serialize};

/// A single Nostr tag value list, e.g. `["d", "daily-brief"]`.
pub type Tag = Vec<String>;

/// A read-only view of a Nostr event, exactly the fields Almanac consumes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEvent {
    /// Event kind (e.g. 30620 for `KIND_WORKFLOW_DEF`, 48050 for schedules).
    pub kind: u32,
    /// Arbitrary content string (markdown body / agent output).
    pub content: String,
    /// Tag list. Almanac reads tags by name prefix (e.g. the first `d` tag).
    pub tags: Vec<Tag>,
    /// `created_at` in unix seconds.
    pub created_at: i64,
}

impl SourceEvent {
    /// Find the first value of the first tag whose name equals `name`.
    ///
    /// `event.first_tag_value("d")` returns the schedule id for a NIP-33 event.
    pub fn first_tag_value(&self, name: &str) -> Option<&str> {
        self.tags
            .iter()
            .find(|t| t.first().map(|n| n == name).unwrap_or(false))
            .and_then(|t| t.get(1).map(|s| s.as_str()))
    }

    /// Find the first value of the first tag whose name has `name` as a
    /// colon-prefixed segment (e.g. `schema_id:research-brief` matches name `schema_id`).
    ///
    /// Returns the value after the colon in the second slot, e.g. for
    /// `["schema_id", "research-brief"]` returns `research-brief`.
    /// Also handles `["schema_id:research-brief"]` (single-segment) form.
    pub fn first_typed_value(&self, name: &str) -> Option<&str> {
        for t in &self.tags {
            let Some(first) = t.first() else { continue };
            // Form A: ["name", "value"]
            if first == name {
                if let Some(v) = t.get(1) {
                    return Some(v.as_str());
                }
            }
            // Form B: ["name:value"]
            if let Some(rest) = first.strip_prefix(&format!("{name}:")) {
                return Some(rest);
            }
        }
        None
    }

    /// All values of all tags named `name`, in order.
    pub fn all_tag_values(&self, name: &str) -> Vec<&str> {
        self.tags
            .iter()
            .filter(|t| t.first().map(|n| n == name).unwrap_or(false))
            .filter_map(|t| t.get(1).map(|s| s.as_str()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(tags: Vec<Tag>) -> SourceEvent {
        SourceEvent {
            kind: 30620,
            content: String::new(),
            tags,
            created_at: 1_700_000_000,
        }
    }

    #[test]
    fn first_tag_value_finds_d_tag() {
        let e = ev(vec![vec!["d".into(), "daily-brief".into()]]);
        assert_eq!(e.first_tag_value("d"), Some("daily-brief"));
        assert_eq!(e.first_tag_value("missing"), None);
    }

    #[test]
    fn typed_value_form_a() {
        let e = ev(vec![vec!["schema_id".into(), "research-brief".into()]]);
        assert_eq!(e.first_typed_value("schema_id"), Some("research-brief"));
    }

    #[test]
    fn typed_value_form_b_colon_prefixed() {
        let e = ev(vec![vec!["schema_id:research-brief".into()]]);
        assert_eq!(e.first_typed_value("schema_id"), Some("research-brief"));
    }

    #[test]
    fn all_tag_values_returns_in_order() {
        let e = ev(vec![
            vec!["alt".into(), "first".into()],
            vec!["alt".into(), "second".into()],
        ]);
        assert_eq!(e.all_tag_values("alt"), vec!["first", "second"]);
    }

    #[test]
    fn empty_tag_handled() {
        let e = ev(vec![vec![]]);
        assert_eq!(e.first_tag_value("d"), None);
    }
}
