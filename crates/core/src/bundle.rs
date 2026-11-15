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

use crate::project::{PROJECT_SCHEMA_VERSION, Project, TrackKind};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub const EXTENSION: &str = "smx";
const PROJECT_FILE: &str = "project.json";
const SUBDIRS: [&str; 4] = ["media", "telemetry", "cache", "history"];
/// Version history keeps this many snapshots; older ones are pruned.
const MAX_SNAPSHOTS: usize = 100;
/// A music copy or voice-over this new may not be saved into the project yet.
const SOUND_GRACE: Duration = Duration::from_secs(10 * 60);

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
    /// If the name is taken, a number is added, to the folder and the project's name alike: "Launch demo 2.smx",
    /// named "Launch demo 2", so the Library and exports tell it from the first (a fifth take was a fifth
    /// "Recording", and its export, numbered by the files in its folder instead, "Recording 3.mp4").
    pub fn create(parent: &Path, project: &mut Project) -> Result<Self, BundleError> {
        fs::create_dir_all(parent)?;
        let base = sanitize_file_name(&project.name);
        let mut root = parent.join(format!("{base}.{EXTENSION}"));
        let mut numbered = None;
        let mut n = 2;
        while root.exists() {
            root = parent.join(format!("{base} {n}.{EXTENSION}"));
            numbered = Some(n);
            n += 1;
            if n > 10_000 {
                return Err(BundleError::AlreadyExists(root));
            }
        }
        if let Some(n) = numbered {
            let name = project.name.trim();
            project.name = format!("{} {n}", if name.is_empty() { base.as_str() } else { name });
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

    /// Where the next voice-over recorded in the editor goes: `media/voice-over 1.m4a`, then 2, and so on, skipping
    /// files that exist. Relative to the project, as tracks name their files.
    pub fn next_voice_over(&self) -> String {
        (1u32..)
            .map(|n| format!("media/voice-over {n}.m4a"))
            .find(|path| !self.root.join(path).exists())
            .unwrap_or_else(|| "media/voice-over.m4a".into())
    }

    /// Deletes the music copies (`media/music.mp3`, `media/music 2.m4a`…) and voice-overs (`media/voice-over 1.m4a`…)
    /// that neither the project nor any version in its history plays, and returns their names. They are kept while
    /// undo may need them: replacing music keeps the old copy, a removed voice-over can come back, and the open
    /// editor's undo history isn't on disk. So this runs when a project is opened, skips files made in the last few
    /// minutes, and deletes nothing if any version can't be read.
    pub fn forget_unused_sounds(&self) -> Result<Vec<String>, BundleError> {
        let music = |project: &Project| -> Vec<String> {
            project
                .tracks
                .iter()
                .filter(|t| matches!(t.kind, TrackKind::Music | TrackKind::VoiceOver))
                .map(|t| t.path.clone())
                .collect()
        };
        let (_, current) = Self::open(&self.root)?;
        let mut used: HashSet<String> = music(&current).into_iter().collect();
        // Every file in history/, read here directly: `history()` skips the ones it can't read, and those might
        // still play a copy.
        for entry in fs::read_dir(self.history_dir())? {
            let name = entry?.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".json") {
                continue;
            }
            let Ok(project) = self.load_snapshot(&name) else { return Ok(Vec::new()) };
            used.extend(music(&project));
        }
        let mut removed = Vec::new();
        let Ok(entries) = fs::read_dir(self.media_dir()) else { return Ok(removed) };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !(is_music_copy(&name) || is_voice_over(&name)) || used.contains(&format!("media/{name}")) {
                continue;
            }
            let recent = entry
                .metadata()
                .and_then(|m| m.modified())
                .map(|at| at.elapsed().map_or(true, |age| age < SOUND_GRACE))
                .unwrap_or(true);
            if !recent && fs::remove_file(entry.path()).is_ok() {
                removed.push(name);
            }
        }
        removed.sort();
        Ok(removed)
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

/// Whether `name` is a voice-over the editor recorded: `voice-over <n>.m4a`.
fn is_voice_over(name: &str) -> bool {
    name.strip_prefix("voice-over ")
        .and_then(|rest| rest.strip_suffix(".m4a"))
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// Whether `name` is one of the copies `import_music` makes: `music.<ext>` or `music <n>.<ext>`.
fn is_music_copy(name: &str) -> bool {
    let Some((stem, ext)) = name.rsplit_once('.') else { return false };
    let number = stem.strip_prefix("music");
    !ext.is_empty()
        && ext.chars().all(|c| c.is_ascii_alphanumeric())
        && number.is_some_and(|n| {
            n.is_empty()
                || n.strip_prefix(' ').is_some_and(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()))
        })
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

        // A second project with the same name gets a number, in its folder and its name.
        let mut again = Project { name: "Launch demo".into(), ..Default::default() };
        let second = ProjectBundle::create(dir.path(), &mut again).unwrap();
        assert!(second.root().ends_with("Launch demo 2.smx"));
        assert_eq!(again.name, "Launch demo 2");
        assert_eq!(ProjectBundle::open(second.root()).unwrap().1.name, "Launch demo 2");
        let mut third = Project { name: "Launch demo".into(), ..Default::default() };
        assert!(ProjectBundle::create(dir.path(), &mut third).unwrap().root().ends_with("Launch demo 3.smx"));
        assert_eq!(third.name, "Launch demo 3");
        assert_eq!(
            ProjectBundle::open(bundle.root()).unwrap().1.name,
            "Launch demo",
            "the first keeps its name"
        );
    }

    #[test]
    fn refuses_projects_from_the_future_and_non_projects() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(ProjectBundle::open(dir.path()), Err(BundleError::NotAProject(_))));
        fs::write(dir.path().join("project.json"), r#"{"schema_version": 999}"#).unwrap();
        assert!(matches!(ProjectBundle::open(dir.path()), Err(BundleError::TooNew { found: 999 })));
    }

    #[test]
    fn voice_overs_take_the_next_free_number_and_are_forgotten_when_nothing_plays_them() {
        use crate::project::MediaTrack;
        let dir = tempfile::tempdir().unwrap();
        let mut project = Project { name: "Narrated".into(), ..Default::default() };
        let bundle = ProjectBundle::create(dir.path(), &mut project).unwrap();
        assert_eq!(bundle.next_voice_over(), "media/voice-over 1.m4a");
        let old = SystemTime::now() - Duration::from_secs(3600);
        for name in ["voice-over 1.m4a", "voice-over 2.m4a", "voice-over x.m4a", "voice-over 3.wav"] {
            let path = bundle.media_dir().join(name);
            fs::write(&path, b"x").unwrap();
            File::options().write(true).open(&path).unwrap().set_modified(old).unwrap();
        }
        assert_eq!(bundle.next_voice_over(), "media/voice-over 3.m4a");
        project.tracks.push(MediaTrack {
            id: "voice-1".into(),
            kind: TrackKind::VoiceOver,
            path: "media/voice-over 2.m4a".into(),
            duration_us: 1_000_000,
            offset_us: 2_000_000,
            volume: 1.0,
            muted: false,
            name: Some("Voice-over 2".into()),
        });
        bundle.save(&mut project).unwrap();
        // Only the voice-over nothing plays goes; files that only look like one stay.
        assert_eq!(bundle.forget_unused_sounds().unwrap(), ["voice-over 1.m4a"]);
        for kept in ["voice-over 2.m4a", "voice-over x.m4a", "voice-over 3.wav"] {
            assert!(bundle.media_dir().join(kept).exists(), "{kept} should stay");
        }
    }

    #[test]
    fn only_music_copies_nothing_plays_are_forgotten() {
        use crate::project::MediaTrack;
        let dir = tempfile::tempdir().unwrap();
        let mut project = Project { name: "Song".into(), ..Default::default() };
        let bundle = ProjectBundle::create(dir.path(), &mut project).unwrap();
        let track = |path: &str| MediaTrack {
            id: path.into(),
            kind: TrackKind::Music,
            path: path.into(),
            duration_us: 1_000_000,
            offset_us: 0,
            volume: 1.0,
            muted: false,
            name: None,
        };
        let old = SystemTime::now() - Duration::from_secs(3600);
        for name in ["music.mp3", "music 2.m4a", "music 3.wav", "music 4.mp3", "screen.mp4", "musicbox.mp3"] {
            let path = bundle.media_dir().join(name);
            fs::write(&path, b"x").unwrap();
            File::options().write(true).open(&path).unwrap().set_modified(old).unwrap();
        }
        // A copy made a moment ago may be an import that isn't saved into the project yet.
        fs::write(bundle.media_dir().join("music 5.mp3"), b"x").unwrap();

        // An earlier version plays music.mp3; the project now plays music 2.m4a.
        project.tracks.push(track("media/music.mp3"));
        bundle.snapshot(&project, "Added music").unwrap();
        project.tracks = vec![track("media/music 2.m4a")];
        bundle.save(&mut project).unwrap();

        assert_eq!(bundle.forget_unused_sounds().unwrap(), ["music 3.wav", "music 4.mp3"]);
        for kept in ["music.mp3", "music 2.m4a", "music 5.mp3", "screen.mp4", "musicbox.mp3"] {
            assert!(bundle.media_dir().join(kept).exists(), "{kept} should stay");
        }

        // A version that can't be read might play anything, so nothing more is deleted.
        fs::write(bundle.media_dir().join("music 6.mp3"), b"x").unwrap();
        File::options()
            .write(true)
            .open(bundle.media_dir().join("music 6.mp3"))
            .unwrap()
            .set_modified(old)
            .unwrap();
        fs::write(bundle.history_dir().join("1-damaged.json"), b"{ not json").unwrap();
        assert!(bundle.forget_unused_sounds().unwrap().is_empty());
        assert!(bundle.media_dir().join("music 6.mp3").exists());
    }

    #[test]
    fn music_copies_are_recognised_by_name() {
        for yes in ["music.mp3", "music 2.m4a", "music 10.flac"] {
            assert!(is_music_copy(yes), "{yes}");
        }
        for no in [
            "musicbox.mp3",
            "music 2b.mp3",
            "music .mp3",
            "music",
            "screen.mp4",
            "my music.mp3",
            "music.m4a.tmp~",
        ] {
            assert!(!is_music_copy(no), "{no}");
        }
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
