//! Control refs and their fingerprints.
//!
//! When an agent plans a take, it names controls by ref (from the accessibility tree) rather than by pixel. Just
//! before the take is performed, every ref is looked up again and compared with what the agent saw. A control that
//! moved is re-aimed automatically; one that disappeared, was disabled or got covered stops the take with a clear
//! reason instead of clicking the wrong thing.

use crate::geometry::Rect;
use crate::hash::Fnv64;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ControlSnapshot {
    /// Stable id within the window, e.g. `toolbar.address` or `w0.c12`.
    pub id: String,
    /// Accessibility role, e.g. `button`, `text_field`, `menu_item`.
    pub role: String,
    pub label: Option<String>,
    pub enabled: bool,
    /// Global screen bounds in points.
    pub bounds: Rect,
    /// The window that owns the control.
    pub window_id: Option<u64>,
    /// Whether another window covers the control's centre.
    #[serde(default)]
    pub occluded: bool,
}

impl ControlSnapshot {
    /// Identity plus geometry, rounded to whole points so sub-point jitter is not a change.
    pub fn fingerprint(&self) -> String {
        let r = self.bounds.rounded();
        let mut h = Fnv64::new();
        h.write_str(&self.id)
            .write_str(&self.role)
            .write_str(self.label.as_deref().unwrap_or(""))
            .write(&[u8::from(self.enabled)])
            .write_i64(r.x as i64)
            .write_i64(r.y as i64)
            .write_i64(r.width as i64)
            .write_i64(r.height as i64);
        format!("{:016x}", h.finish())
    }

    /// A short human name for the control: its label, or its role.
    pub fn name(&self) -> String {
        match self.label.as_deref().map(str::trim).filter(|l| !l.is_empty()) {
            Some(label) => format!("“{label}” {}", self.role.replace('_', " ")),
            None => self.role.replace('_', " "),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RefStatus {
    Unchanged,
    /// Same control, new place. The take re-aims at the new position.
    Moved {
        dx: f64,
        dy: f64,
    },
    /// Same control, different size. The take re-aims at the new centre.
    Resized,
    Disabled,
    Covered,
    Gone,
    /// The label changed, which usually means a different control now has this id.
    Relabelled {
        from: Option<String>,
        to: Option<String>,
    },
}

impl RefStatus {
    pub fn blocks_take(&self) -> bool {
        matches!(self, Self::Disabled | Self::Covered | Self::Gone | Self::Relabelled { .. })
    }

    /// A sentence for the agent and the take card.
    pub fn describe(&self, control: &ControlSnapshot) -> String {
        let name = control.name();
        match self {
            Self::Unchanged => format!("{name} is ready"),
            Self::Moved { dx, dy } => format!("{name} moved by {dx:.0}, {dy:.0} points; re-aimed"),
            Self::Resized => format!("{name} changed size; re-aimed at its centre"),
            Self::Disabled => format!("{name} is disabled now, so the take stopped before clicking it"),
            Self::Covered => format!("{name} is covered by another window, so the take stopped"),
            Self::Gone => format!("{name} is no longer on screen, so the take stopped"),
            Self::Relabelled { from, to } => format!(
                "the control at this ref changed from {} to {}, so the take stopped",
                from.as_deref().unwrap_or("(no label)"),
                to.as_deref().unwrap_or("(no label)")
            ),
        }
    }
}

/// Compares what the agent planned against what is on screen now.
pub fn check(planned: &ControlSnapshot, current: Option<&ControlSnapshot>) -> RefStatus {
    let Some(current) = current else {
        return RefStatus::Gone;
    };
    if planned.role != current.role || planned.label != current.label {
        return RefStatus::Relabelled { from: planned.label.clone(), to: current.label.clone() };
    }
    if !current.enabled {
        return RefStatus::Disabled;
    }
    if current.occluded {
        return RefStatus::Covered;
    }
    let (a, b) = (planned.bounds.rounded(), current.bounds.rounded());
    if (a.width - b.width).abs() >= 1.0 || (a.height - b.height).abs() >= 1.0 {
        return RefStatus::Resized;
    }
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    if dx.abs() >= 1.0 || dy.abs() >= 1.0 {
        return RefStatus::Moved { dx, dy };
    }
    RefStatus::Unchanged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control() -> ControlSnapshot {
        ControlSnapshot {
            id: "toolbar.export".into(),
            role: "button".into(),
            label: Some("Export".into()),
            enabled: true,
            bounds: Rect::new(100.0, 40.0, 80.0, 28.0),
            window_id: Some(7),
            occluded: false,
        }
    }

    #[test]
    fn fingerprint_ignores_sub_point_jitter() {
        let a = control();
        let mut b = control();
        b.bounds.x += 0.3;
        assert_eq!(a.fingerprint(), b.fingerprint());
        b.bounds.x += 2.0;
        assert_ne!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn statuses() {
        let planned = control();
        assert_eq!(check(&planned, Some(&planned)), RefStatus::Unchanged);
        assert_eq!(check(&planned, None), RefStatus::Gone);

        let mut moved = control();
        moved.bounds.x += 30.0;
        assert_eq!(check(&planned, Some(&moved)), RefStatus::Moved { dx: 30.0, dy: 0.0 });
        assert!(!check(&planned, Some(&moved)).blocks_take());

        let mut disabled = control();
        disabled.enabled = false;
        assert!(check(&planned, Some(&disabled)).blocks_take());

        let mut covered = control();
        covered.occluded = true;
        assert_eq!(check(&planned, Some(&covered)), RefStatus::Covered);

        let mut other = control();
        other.label = Some("Delete".into());
        let status = check(&planned, Some(&other));
        assert!(status.blocks_take());
        assert!(status.describe(&planned).contains("Delete"));
    }
}
