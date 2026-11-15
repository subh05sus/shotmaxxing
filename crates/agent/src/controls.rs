//! Naming the controls an agent can point at, the same way on every system. Ids are made from what a control is and
//! says (`button.export`, `text_field.search`, `link.docs-2`), so they stay the same when unrelated parts of the
//! window change. Field contents are never read, only labels.

use std::collections::HashMap;

use shotmaxxing_core::refs::ControlSnapshot;

/// Most controls listed for one window.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
pub const LIMIT: usize = 800;
/// Most elements visited in one walk; web pages can hold many thousands.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
pub const MAX_VISITS: usize = 6_000;
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
pub const MAX_DEPTH: usize = 48;

/// A label the way a take shows it: whitespace collapsed and at most 80 characters, or `None` when it's blank.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
pub fn clean_label(text: &str) -> Option<String> {
    let words = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if words.is_empty() {
        return None;
    }
    Some(if words.chars().count() > 80 { words.chars().take(80).collect() } else { words })
}

fn slug(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            out.push(ch);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.chars().count() >= 32 {
            break;
        }
    }
    out.trim_end_matches('-').to_string()
}

/// `button.export`, then `button.export-2` for a second one; `button-1`, `button-2` for buttons with no label.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
pub fn make_id(role: &str, label: Option<&str>, taken: &mut HashMap<String, usize>) -> String {
    let slug = label.map(slug).filter(|s| !s.is_empty());
    let base = match &slug {
        Some(s) => format!("{role}.{s}"),
        None => role.to_string(),
    };
    let count = taken.entry(base.clone()).or_insert(0);
    *count += 1;
    if slug.is_some() && *count == 1 { base } else { format!("{base}-{count}") }
}

/// An exact id, then an exact label, then the shortest label containing the words; enabled controls first.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
pub fn best_match<'a>(controls: &'a [ControlSnapshot], query: &str) -> Option<&'a ControlSnapshot> {
    let q = query.trim().to_lowercase();
    let label = |c: &ControlSnapshot| c.label.as_deref().unwrap_or("").to_lowercase();
    let pick = |matches: &dyn Fn(&ControlSnapshot) -> bool| {
        controls.iter().filter(|c| matches(c)).min_by_key(|c| (!c.enabled, label(c).len()))
    };
    pick(&|c| c.id.to_lowercase() == q)
        .or_else(|| pick(&|c| label(c) == q))
        .or_else(|| pick(&|c| !q.is_empty() && label(c).contains(&q)))
}

#[cfg(test)]
mod tests {
    use shotmaxxing_core::geometry::Rect;

    use super::*;

    #[test]
    fn ids_read_like_the_control() {
        let mut taken = HashMap::new();
        assert_eq!(make_id("button", Some("Export…"), &mut taken), "button.export");
        assert_eq!(make_id("button", Some("Export…"), &mut taken), "button.export-2");
        assert_eq!(make_id("button", None, &mut taken), "button-1");
        assert_eq!(make_id("button", None, &mut taken), "button-2");
        assert_eq!(
            make_id("text_field", Some("Search or enter website name"), &mut taken),
            "text_field.search-or-enter-website-name"
        );
        assert_eq!(slug("Ünïcode & Co."), "ünïcode-co");
    }

    #[test]
    fn labels_are_tidied_and_kept_short() {
        assert_eq!(clean_label("  Save \n as… "), Some("Save as…".into()));
        assert_eq!(clean_label(" \t "), None);
        assert_eq!(clean_label(&"a".repeat(100)).map(|l| l.chars().count()), Some(80));
    }

    #[test]
    fn finds_by_id_then_label_then_words() {
        let c = |id: &str, label: &str, enabled| ControlSnapshot {
            id: id.into(),
            role: "button".into(),
            label: Some(label.into()),
            enabled,
            bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
            window_id: None,
            occluded: false,
        };
        let controls = [
            c("button.save-as", "Save As…", true),
            c("button.save", "Save", false),
            c("button.save-2", "Save", true),
        ];
        assert_eq!(best_match(&controls, "button.save").unwrap().id, "button.save");
        assert_eq!(best_match(&controls, "save").unwrap().id, "button.save-2", "enabled first");
        assert_eq!(best_match(&controls, "as").unwrap().id, "button.save-as");
        assert!(best_match(&controls, "export").is_none());
    }
}
