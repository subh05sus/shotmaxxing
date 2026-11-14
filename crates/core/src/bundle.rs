//! `.smx` project bundles on disk.
//!
//! ```text
//! Launch demo.smx/
//! ├ project.json   the only file that changes while editing
//! ├ media/         recordings (fragmented MP4 / M4A)
//! ├ telemetry/     cursor.bin, events.jsonl
//! ├ cache/         thumbnails, scrub sprites, waveforms (safe to delete)
//! └ history/       snapshots for Version history
//! ```
//!
//! Saves are atomic: the new `project.json` is written to a temporary file, flushed to disk, then renamed over the
//! old one. A crash mid-save leaves the previous version intact.

use crate::project::{PROJECT_SCHEMA_VERSION, Project};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub const EXTENSION: &str = "smx";
const PROJECT_FILE: &str = "project.json";
const SUBDIRS: [&str; 4] = ["media", "telemetry", "cache", "history"];
/// Version history keeps this many snapshots; older ones are pruned.
const MAX_SNAPSHOTS: usize = 100;

#[derive(Debug, Error)]
pub enum BundleError {
    #[error("{0} is not a Shotmaxxing project (no project.json inside)")]
    NotAProject(PathBuf),
    #[error("a project named {0} already exists here")]
    AlreadyExists(PathBuf),
    #[error("this project was made by a newer Shotmaxxing (format {found}); update the app to open it")]
    TooNew { found: u32 },
    #[error("project.json is damaged: {0}")]
    Damaged(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug)]
pub struct ProjectBundle {
    root: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub file: String,
    pub saved_at_ms: u64,
    pub label: String,
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

impl ProjectBundle {
    /// Creates `<parent>/<name>.smx` with its folders and an initial `project.json`.
    /// If the name is taken, a number is added: "Launch demo 2.smx".
    pub fn create(parent: &Path, project: &mut Project) -> Result<Self, BundleError> {
        fs::create_dir_all(parent)?;
        let base = sanitize_file_name(&project.name);
        let mut root = parent.join(format!("{base}.{EXTENSION}"));
        let mut n = 2;
        while root.exists() {
            root = parent.join(format!("{base} {n}.{EXTENSION}"));
            n += 1;
            if n > 10_000 {
                return Err(BundleError::AlreadyExists(root));
            }
        }
        fs::create_dir(&root)?;
        for dir in SUBDIRS {
            fs::create_dir(root.join(dir))?;
        }
        let bundle = Self { root };
        let now = now_ms();
        if project.created_at_ms == 0 {
            project.created_at_ms = now;
        }
        if project.id.is_empty() {
            project.id =
                format!("{now:x}-{:x}", crate::hash::mix64(now ^ std::process::id() as u64) & 0xffff_ffff);
        }
        bundle.save(project)?;
        Ok(bundle)
    }

    pub fn open(root: impl Into<PathBuf>) -> Result<(Self, Project), BundleError> {
        let root = root.into();
        let path = root.join(PROJECT_FILE);
        if !path.is_file() {
            return Err(BundleError::NotAProject(root));
        }
        let text = fs::read_to_string(&path)?;
        let value: serde_json::Value = serde_json::from_str(&text)?;
        let found = value.get("schema_version").and_then(serde_json::Value::as_u64).unwrap_or(1) as u32;
        if found > PROJECT_SCHEMA_VERSION {
            return Err(BundleError::TooNew { found });
        }
        let project: Project = serde_json::from_value(migrate(value, found))?;
        let bundle = Self { root };
        for dir in SUBDIRS {
            fs::create_dir_all(bundle.root.join(dir))?;
        }
        Ok((bundle, project))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn media_dir(&self) -> PathBuf {
        self.root.join("media")
    }
    pub fn telemetry_dir(&self) -> PathBuf {
        self.root.join("telemetry")
    }
    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }
    pub fn history_dir(&self) -> PathBuf {
        self.root.join("history")
    }

    /// Atomically writes `project.json`.
    pub fn save(&self, project: &mut Project) -> Result<(), BundleError> {
        project.schema_version = PROJECT_SCHEMA_VERSION;
        project.updated_at_ms = now_ms();
        let json = serde_json::to_vec_pretty(project)?;
        write_atomic(&self.root.join(PROJECT_FILE), &json)?;
        Ok(())
    }

    /// Saves a copy of the project into `history/` with a short label ("Added zooms").
    pub fn snapshot(&self, project: &Project, label: &str) -> Result<Snapshot, BundleError> {
        let saved_at_ms = now_ms();
        let mut file =
            format!("{saved_at_ms}-{}.json", sanitize_file_name(label).replace(' ', "-").to_lowercase());
        let mut n = 2;
        while self.history_dir().join(&file).exists() {
            file = format!("{saved_at_ms}-{n}.json");
            n += 1;
        }
        let record = serde_json::json!({ "label": label, "saved_at_ms": saved_at_ms, "project": project });
        write_atomic(&self.history_dir().join(&file), &serde_json::to_vec(&record)?)?;
        self.prune_history()?;
        Ok(Snapshot { file, saved_at_ms, label: label.to_string() })
    }

    /// Newest first.
    pub fn history(&self) -> Result<Vec<Snapshot>, BundleError> {
        let mut out = Vec::new();
        for entry in fs::read_dir(self.history_dir())? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".json") {
                continue;
            }
            let Ok(text) = fs::read_to_string(entry.path()) else { continue };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
            out.push(Snapshot {
                file: name,
                saved_at_ms: value.get("saved_at_ms").and_then(serde_json::Value::as_u64).unwrap_or(0),
                label: value.get("label").and_then(serde_json::Value::as_str).unwrap_or("").to_string(),
            });
        }
        out.sort_by(|a, b| b.saved_at_ms.cmp(&a.saved_at_ms).then(b.file.cmp(&a.file)));
        Ok(out)
    }

    pub fn load_snapshot(&self, file: &str) -> Result<Project, BundleError> {
        // Only plain file names inside history/ are allowed.
        if file.contains(['/', '\\']) || file.starts_with('.') {
            return Err(BundleError::NotAProject(self.history_dir().join(file)));
        }
        let text = fs::read_to_string(self.history_dir().join(file))?;
        let value: serde_json::Value = serde_json::from_str(&text)?;
        let project = value.get("project").cloned().unwrap_or(value);
        Ok(serde_json::from_value(project)?)
    }

    fn prune_history(&self) -> Result<(), BundleError> {
        let history = self.history()?;
        for old in history.iter().skip(MAX_SNAPSHOTS) {
            let _ = fs::remove_file(self.history_dir().join(&old.file));
        }
        Ok(())
    }

    /// Deletes everything in `cache/`. It is rebuilt on demand.
    pub fn clear_cache(&self) -> Result<(), BundleError> {
        let cache = self.cache_dir();
        if cache.exists() {
            fs::remove_dir_all(&cache)?;
        }
        fs::create_dir_all(cache)?;
        Ok(())
    }
}

/// Upgrades older project JSON to the current schema. Version 1 is current, so this is the identity for now;
/// each future version adds one step here.
fn migrate(value: serde_json::Value, _from: u32) -> serde_json::Value {
    value
}

/// Writes a file so that readers see either the old or the new contents, never a partial file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let tmp = dir.join(format!(".{file_name}.{}.tmp", std::process::id()));
    {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

/// Makes a name safe as a file name on macOS and Windows.
pub fn sanitize_file_name(name: &str) -> String {
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
        "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let cleaned: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_matches(|c: char| c == '.' || c == ' ');
    let mut out: String = trimmed.chars().take(120).collect();
    if out.is_empty() {
        out = "Untitled recording".into();
    }
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(&out)) {
        out.push_str(" recording");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_save_open_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut project = Project { name: "Launch demo".into(), ..Default::default() };
        let bundle = ProjectBundle::create(dir.path(), &mut project).unwrap();
        assert!(bundle.root().ends_with("Launch demo.smx"));
        assert!(bundle.media_dir().is_dir() && bundle.history_dir().is_dir());
        assert!(!project.id.is_empty());

        project.canvas.padding = 0.12;
        bundle.save(&mut project).unwrap();
        let (_, reopened) = ProjectBundle::open(bundle.root()).unwrap();
        assert_eq!(reopened.canvas.padding, 0.12);
        assert_eq!(reopened.id, project.id);

        // A second project with the same name gets a number.
        let mut again = Project { name: "Launch demo".into(), ..Default::default() };
        let second = ProjectBundle::create(dir.path(), &mut again).unwrap();
        assert!(second.root().ends_with("Launch demo 2.smx"));
    }

    #[test]
    fn refuses_projects_from_the_future_and_non_projects() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(ProjectBundle::open(dir.path()), Err(BundleError::NotAProject(_))));
        fs::write(dir.path().join("project.json"), r#"{"schema_version": 999}"#).unwrap();
        assert!(matches!(ProjectBundle::open(dir.path()), Err(BundleError::TooNew { found: 999 })));
    }

    #[test]
    fn history_snapshots_restore() {
        let dir = tempfile::tempdir().unwrap();
        let mut project = Project { name: "Demo".into(), ..Default::default() };
        let bundle = ProjectBundle::create(dir.path(), &mut project).unwrap();
        let snap = bundle.snapshot(&project, "Trimmed start").unwrap();
        project.canvas.padding = 0.2;
        bundle.snapshot(&project, "Added padding").unwrap();
        let history = bundle.history().unwrap();
        assert_eq!(history.len(), 2);
        let restored = bundle.load_snapshot(&snap.file).unwrap();
        assert_eq!(restored.canvas.padding, Project::default().canvas.padding);
        assert!(bundle.load_snapshot("../project.json").is_err());
    }

    #[test]
    fn file_names_are_safe_everywhere() {
        assert_eq!(sanitize_file_name("a/b:c*d?"), "a b c d");
        assert_eq!(sanitize_file_name("   "), "Untitled recording");
        assert_eq!(sanitize_file_name("con"), "con recording");
        assert_eq!(sanitize_file_name("..hidden.."), "hidden");
    }
}
