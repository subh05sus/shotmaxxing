//! Content packs: wallpapers and looks, shared as one small JSON file.
//!
//! A person adds a pack in Settings, and its wallpapers and looks show in the editor's Background tool. A pack is
//! only data. Its wallpapers are gradients in the same CSS subset as the built-in ones ([`wallpaper::check`]), so the
//! exporter draws them exactly as the editor shows them, and nothing in a pack can fetch anything or run. A project
//! keeps a copy of the wallpaper it uses, so it still exports the same without the pack.
//!
//! ```json
//! {
//!   "shotmaxxing_pack": 1,
//!   "id": "northwind",
//!   "name": "Northwind",
//!   "author": "Northwind Labs",
//!   "wallpapers": [
//!     { "id": "sunrise", "name": "Sunrise", "css": "linear-gradient(135deg, oklch(0.82 0.12 75), oklch(0.55 0.17 35))" }
//!   ],
//!   "looks": [
//!     { "id": "launch", "name": "Launch", "wallpaper": "sunrise", "padding": 0.1, "corner_radius": 16, "shadow": 0.6 }
//!   ]
//! }
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::wallpaper;

/// The pack format this app reads.
pub const FORMAT: u64 = 1;
/// The largest pack file read.
pub const MOST_BYTES: u64 = 256 * 1024;
const MOST_WALLPAPERS: usize = 48;
const MOST_LOOKS: usize = 24;
/// The canvas shapes a look can choose, as the editor names them.
pub const ASPECTS: [&str; 5] = ["16:9", "4:3", "1:1", "9:16", "original"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    pub shotmaxxing_pack: u64,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default)]
    pub wallpapers: Vec<PackWallpaper>,
    #[serde(default)]
    pub looks: Vec<Look>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackWallpaper {
    pub id: String,
    pub name: String,
    pub css: String,
}

/// A look: some or all of the Background tool's settings, applied together.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Look {
    pub id: String,
    pub name: String,
    /// A wallpaper of this pack, by its id here, or a built-in one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wallpaper: Option<String>,
    /// Space around the recording, as a fraction of the canvas's shorter side (0 to 0.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<f64>,
    /// In pixels at 1080p (0 to 40).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_radius: Option<f64>,
    /// How strong the shadow is (0 to 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<f64>,
    /// One of [`ASPECTS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aspect: Option<String>,
}

impl Pack {
    /// The id a project uses for one of this pack's wallpapers: `<pack>.<wallpaper>`, so packs never clash with each
    /// other or with the built-in wallpapers (whose ids have no dot).
    pub fn wallpaper_id(&self, wallpaper: &str) -> String {
        format!("{}.{wallpaper}", self.id)
    }

    /// A look's wallpaper as a project names it, with its CSS when it's this pack's.
    pub fn look_wallpaper(&self, look: &Look) -> Option<(String, Option<String>)> {
        let id = look.wallpaper.as_deref()?;
        Some(match self.wallpapers.iter().find(|w| w.id == id) {
            Some(own) => (self.wallpaper_id(&own.id), Some(own.css.clone())),
            None => (id.to_string(), None),
        })
    }

    /// Everything a pack must be before it's added: see [`read`].
    fn check(&self) -> Result<(), String> {
        short_id(&self.id).map_err(|e| format!("The pack's id {e}"))?;
        named(&self.name, 60).map_err(|e| format!("The pack's name {e}"))?;
        if let Some(author) = &self.author {
            named(author, 60).map_err(|e| format!("The author {e}"))?;
        }
        if self.wallpapers.is_empty() && self.looks.is_empty() {
            return Err("The pack has no wallpapers or looks.".into());
        }
        if self.wallpapers.len() > MOST_WALLPAPERS || self.looks.len() > MOST_LOOKS {
            return Err(format!("A pack holds at most {MOST_WALLPAPERS} wallpapers and {MOST_LOOKS} looks."));
        }
        for (i, w) in self.wallpapers.iter().enumerate() {
            let which = || format!("Wallpaper {} (“{}”)", i + 1, w.name);
            short_id(&w.id).map_err(|e| format!("{}: its id {e}", which()))?;
            named(&w.name, 40).map_err(|e| format!("{}: its name {e}", which()))?;
            if self.wallpapers[..i].iter().any(|o| o.id == w.id) {
                return Err(format!("{}: another wallpaper already has the id “{}”.", which(), w.id));
            }
            wallpaper::check(&w.css).map_err(|e| format!("{}: its css {e}.", which()))?;
        }
        let built_in = wallpaper::wallpapers();
        for (i, look) in self.looks.iter().enumerate() {
            let which = || format!("Look {} (“{}”)", i + 1, look.name);
            short_id(&look.id).map_err(|e| format!("{}: its id {e}", which()))?;
            named(&look.name, 40).map_err(|e| format!("{}: its name {e}", which()))?;
            if self.looks[..i].iter().any(|o| o.id == look.id) {
                return Err(format!("{}: another look already has the id “{}”.", which(), look.id));
            }
            if let Some(id) = &look.wallpaper
                && !self.wallpapers.iter().any(|w| &w.id == id)
                && !built_in.iter().any(|w| &w.id == id)
            {
                return Err(format!("{}: there's no wallpaper “{id}” in this pack or built in.", which()));
            }
            let within = |value: Option<f64>, most: f64, what: &str| match value {
                Some(v) if !(v.is_finite() && (0.0..=most).contains(&v)) => {
                    Err(format!("{}: {what} goes from 0 to {most}, not {v}.", which()))
                }
                _ => Ok(()),
            };
            within(look.padding, 0.2, "padding")?;
            within(look.corner_radius, 40.0, "corner_radius")?;
            within(look.shadow, 1.0, "shadow")?;
            if let Some(aspect) = &look.aspect
                && !ASPECTS.contains(&aspect.as_str())
            {
                return Err(format!("{}: aspect is one of {}, not “{aspect}”.", which(), ASPECTS.join(", ")));
            }
            let sets = look.wallpaper.is_some()
                || look.padding.is_some()
                || look.corner_radius.is_some()
                || look.shadow.is_some()
                || look.aspect.is_some();
            if !sets {
                return Err(format!("{}: it doesn't set anything.", which()));
            }
        }
        Ok(())
    }
}

/// Reads and checks a pack: the format this app knows, ids of lowercase letters, digits and dashes, names that fit,
/// wallpapers [`wallpaper::check`] accepts, and looks within the editor's ranges. The error says what to fix.
pub fn read(text: &str) -> Result<Pack, String> {
    if text.len() as u64 > MOST_BYTES {
        return Err("The file is too big for a content pack (256 KB at most).".into());
    }
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("The file isn't valid JSON: {e}."))?;
    match value.get("shotmaxxing_pack").and_then(serde_json::Value::as_u64) {
        None => {
            return Err("This isn't a Shotmaxxing content pack (it has no \"shotmaxxing_pack\": 1).".into());
        }
        Some(format) if format > FORMAT => {
            return Err(
                "This pack is for a newer version of Shotmaxxing. Update the app, then add it again.".into(),
            );
        }
        Some(_) => {}
    }
    let pack: Pack = serde_json::from_value(value).map_err(|e| format!("The pack couldn't be read: {e}."))?;
    pack.check()?;
    Ok(pack)
}

/// Reads and checks the pack in `path`.
pub fn read_file(path: &Path) -> Result<Pack, String> {
    let size = std::fs::metadata(path).map_err(|e| format!("Couldn't open the file: {e}."))?.len();
    if size > MOST_BYTES {
        return Err("The file is too big for a content pack (256 KB at most).".into());
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("Couldn't read the file: {e}."))?;
    read(&text)
}

/// Where added packs are kept: a `packs` folder in the app's data folder (`~/Library/Application Support/<app id>` on
/// macOS, `%APPDATA%\<app id>` on Windows), so the app and the `shotmaxxing` command find the same ones.
pub fn dir() -> Option<PathBuf> {
    let app = shotmaxxing_core::APP_ID;
    #[cfg(target_os = "macos")]
    let base = PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support");
    #[cfg(windows)]
    let base = PathBuf::from(std::env::var_os("APPDATA")?);
    #[cfg(not(any(target_os = "macos", windows)))]
    let base = match std::env::var_os("XDG_DATA_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var_os("HOME")?).join(".local/share"),
    };
    Some(base.join(app).join("packs"))
}

/// Every pack in `dir`, by name. A file that isn't a valid pack is left out, with the reason.
pub fn installed_in(dir: &Path) -> (Vec<Pack>, Vec<(PathBuf, String)>) {
    let (mut packs, mut skipped) = (Vec::new(), Vec::new());
    let Ok(entries) = std::fs::read_dir(dir) else { return (packs, skipped) };
    for path in entries.flatten().map(|e| e.path()) {
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        match read_file(&path) {
            Ok(pack) if !packs.iter().any(|p: &Pack| p.id == pack.id) => packs.push(pack),
            Ok(pack) => skipped.push((path, format!("another pack already has the id “{}”", pack.id))),
            Err(reason) => skipped.push((path, reason)),
        }
    }
    packs.sort_by_key(|pack| pack.name.to_lowercase());
    (packs, skipped)
}

/// The packs added on this computer.
pub fn installed() -> Vec<Pack> {
    dir().map(|dir| installed_in(&dir).0).unwrap_or_default()
}

/// Adds the pack in `source` to `dir`, replacing an earlier version of the same pack. Returns it, and whether it
/// replaced one.
pub fn add(dir: &Path, source: &Path) -> Result<(Pack, bool), String> {
    let pack = read_file(source)?;
    let replaced = put(dir, &pack)?;
    Ok((pack, replaced))
}

/// Saves `pack` in `dir` (checked again first, since it may come from the UI), replacing an earlier version of it.
/// Returns whether it replaced one. Removing a pack can be undone this way.
pub fn put(dir: &Path, pack: &Pack) -> Result<bool, String> {
    let text = serde_json::to_string_pretty(pack).map_err(|e| e.to_string())?;
    let pack = read(&text)?;
    std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't make the packs folder: {e}."))?;
    let target = dir.join(format!("{}.json", pack.id));
    let replaced = target.exists();
    // Written whole, then moved into place, so a half-written pack is never read.
    let temporary = dir.join(format!(".{}.json.part", pack.id));
    std::fs::write(&temporary, text).map_err(|e| format!("Couldn't save the pack: {e}."))?;
    std::fs::rename(&temporary, &target).map_err(|e| {
        let _ = std::fs::remove_file(&temporary);
        format!("Couldn't save the pack: {e}.")
    })?;
    Ok(replaced)
}

/// Takes the pack `id` out of `dir`. Projects that use its wallpapers keep their own copies.
pub fn remove(dir: &Path, id: &str) -> Result<(), String> {
    short_id(id).map_err(|e| format!("The pack's id {e}"))?;
    match std::fs::remove_file(dir.join(format!("{id}.json"))) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Couldn't remove the pack: {e}.")),
    }
}

/// An id: 1 to 40 lowercase letters, digits and dashes, starting with a letter or digit.
fn short_id(id: &str) -> Result<(), String> {
    let fits = (1..=40).contains(&id.len())
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-');
    if fits {
        Ok(())
    } else {
        Err(format!("“{id}” should be 1 to 40 lowercase letters, digits and dashes."))
    }
}

fn named(name: &str, most: usize) -> Result<(), String> {
    let length = name.trim().chars().count();
    if length == 0 || length > most || name.chars().any(char::is_control) {
        return Err(format!("should be 1 to {most} characters on one line."));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUNRISE: &str = "linear-gradient(135deg, oklch(0.82 0.12 75), oklch(0.55 0.17 35))";

    fn pack(extra: &str) -> String {
        format!(
            r#"{{"shotmaxxing_pack": 1, "id": "northwind", "name": "Northwind",
                "wallpapers": [{{"id": "sunrise", "name": "Sunrise", "css": "{SUNRISE}"}}]{extra}}}"#
        )
    }

    #[test]
    fn a_pack_reads_and_names_its_wallpapers() {
        let read = read(&pack(
            r#", "looks": [{"id": "launch", "name": "Launch", "wallpaper": "sunrise", "padding": 0.1}]"#,
        ))
        .unwrap();
        assert_eq!(read.wallpaper_id("sunrise"), "northwind.sunrise");
        let look = &read.looks[0];
        assert_eq!(read.look_wallpaper(look), Some(("northwind.sunrise".into(), Some(SUNRISE.into()))));
        // A look can use a built-in wallpaper too.
        let built_in = read_ok(r#", "looks": [{"id": "dark", "name": "Dark", "wallpaper": "graphite"}]"#);
        assert_eq!(built_in.look_wallpaper(&built_in.looks[0]), Some(("graphite".into(), None)));
    }

    fn read_ok(extra: &str) -> Pack {
        read(&pack(extra)).unwrap_or_else(|e| panic!("{e}"))
    }

    fn refusal(text: &str) -> String {
        read(text).expect_err("should be refused")
    }

    #[test]
    fn the_documented_example_checks_out() {
        let example = read(include_str!("../../../docs/content-packs/northwind.json")).unwrap();
        assert_eq!((example.wallpapers.len(), example.looks.len()), (3, 3));
    }

    #[test]
    fn what_isnt_a_pack_is_refused_with_the_reason() {
        assert!(refusal("not json").contains("isn't valid JSON"));
        assert!(refusal(r#"{"id": "x"}"#).contains("isn't a Shotmaxxing content pack"));
        assert!(refusal(r#"{"shotmaxxing_pack": 2, "id": "x"}"#).contains("newer version"));
        assert!(refusal(&pack(r#", "cursors": []"#)).contains("unknown field"));
        assert!(
            refusal(r#"{"shotmaxxing_pack": 1, "id": "Bad Id", "name": "x", "looks": []}"#)
                .contains("lowercase")
        );
        assert!(
            refusal(r#"{"shotmaxxing_pack": 1, "id": "empty", "name": "Empty"}"#).contains("no wallpapers")
        );
    }

    #[test]
    fn wallpapers_must_draw_the_same_in_the_editor_and_the_export() {
        let with_css = |css: &str| {
            format!(
                r#"{{"shotmaxxing_pack": 1, "id": "p", "name": "P",
                    "wallpapers": [{{"id": "w", "name": "W", "css": "{css}"}}]}}"#
            )
        };
        // Nothing that could fetch, reach past the background or isn't drawn the same.
        assert!(refusal(&with_css("url(//example.com/a.png)")).contains("unsupported"));
        assert!(refusal(&with_css("linear-gradient(red, blue)")).contains("css"));
        assert!(refusal(&with_css("linear-gradient(#fff, #000)")).contains("“#”"));
        assert!(refusal(&with_css(&format!("{SUNRISE}; position: fixed"))).contains("“;”"));
        // Gradients that leave the picture see-through, or stack too many layers.
        let see_through = "radial-gradient(50% 50% at 50% 50%, oklch(0.8 0.1 80) 0%, transparent 70%)";
        assert!(refusal(&with_css(see_through)).contains("see-through"));
        let layers = [SUNRISE; 9].join(", ");
        assert!(refusal(&with_css(&layers)).contains("8 at most"));
        // Duplicates.
        let twice = r#"{"shotmaxxing_pack": 1, "id": "p", "name": "P", "wallpapers": [
            {"id": "w", "name": "One", "css": "linear-gradient(oklch(0.5 0.1 20), oklch(0.3 0.1 20))"},
            {"id": "w", "name": "Two", "css": "linear-gradient(oklch(0.5 0.1 20), oklch(0.3 0.1 20))"}]}"#;
        assert!(refusal(twice).contains("already has the id"));
    }

    #[test]
    fn looks_stay_within_the_editors_ranges() {
        let look = |fields: &str| pack(&format!(r#", "looks": [{{"id": "l", "name": "L", {fields}}}]"#));
        assert!(refusal(&look(r#""padding": 0.5"#)).contains("padding goes from 0 to 0.2"));
        assert!(refusal(&look(r#""corner_radius": -1"#)).contains("corner_radius"));
        assert!(refusal(&look(r#""shadow": 2"#)).contains("shadow"));
        assert!(refusal(&look(r#""aspect": "2:1""#)).contains("aspect is one of"));
        assert!(refusal(&look(r#""wallpaper": "nowhere""#)).contains("no wallpaper “nowhere”"));
        assert!(refusal(&pack(r#", "looks": [{"id": "l", "name": "L"}]"#)).contains("doesn't set anything"));
        read_ok(r#", "looks": [{"id": "l", "name": "L", "aspect": "9:16", "shadow": 0}]"#);
    }

    #[test]
    fn packs_are_added_replaced_listed_and_removed() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join("packs");
        let source = home.path().join("northwind.json");
        std::fs::write(&source, pack("")).unwrap();
        let (added, replaced) = add(&dir, &source).unwrap();
        assert_eq!((added.id.as_str(), replaced), ("northwind", false));
        assert!(add(&dir, &source).unwrap().1, "adding it again replaces it");
        // A file that isn't a pack is left out of the list, with the reason.
        std::fs::write(dir.join("broken.json"), "{").unwrap();
        let (packs, skipped) = installed_in(&dir);
        assert_eq!(packs.len(), 1);
        assert_eq!(skipped.len(), 1);
        remove(&dir, "northwind").unwrap();
        remove(&dir, "northwind").unwrap();
        assert!(installed_in(&dir).0.is_empty());
        // Undoing a removal puts it back, checked again.
        assert!(!put(&dir, &added).unwrap());
        assert_eq!(installed_in(&dir).0, vec![added.clone()]);
        let broken = Pack { id: "Not An Id".into(), ..added };
        assert!(put(&dir, &broken).is_err());
        assert!(remove(&dir, "../escape").is_err());
    }
}
