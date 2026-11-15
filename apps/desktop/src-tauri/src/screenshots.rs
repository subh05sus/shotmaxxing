//! Screenshots: taken from the record dialog (⌥⇧4 opens it ready for one) and saved as projects that open in the
//! editor. There they're styled and annotated like a recording, then copied or saved as an image.

use std::path::Path;
use std::time::Duration;

use serde::Deserialize;
use shotmaxxing_capture::Target;
use shotmaxxing_core::project::{Background, Canvas, Shadow};
use tauri::{AppHandle, Manager, Runtime};

use crate::recording::{parse_source, projects_folder};

/// How long the record dialog is given to leave the screen before a picture of the whole screen is taken.
const STEP_ASIDE: Duration = Duration::from_millis(250);

/// The look someone gave their last screenshot, which the next one starts from, in the project's own units.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Look {
    wallpaper: String,
    /// The wallpaper itself, when it came from a content pack.
    #[serde(default)]
    wallpaper_css: Option<String>,
    padding: f64,
    corner_radius: f64,
    shadow_opacity: f64,
}

impl Look {
    /// The look as a canvas, or `None` when it isn't one the editor could have made (an old or hand-edited setting).
    /// A content pack's wallpaper (`<pack>.<wallpaper>`) comes with its CSS, which must check out.
    fn canvas(&self) -> Option<Canvas> {
        let id = |part: &str| {
            !part.is_empty()
                && part.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        };
        let css = match (self.wallpaper.split_once('.'), &self.wallpaper_css) {
            (None, None) => None,
            (Some((pack, name)), Some(css))
                if id(pack) && id(name) && shotmaxxing_render::wallpaper::check(css).is_ok() =>
            {
                Some(css.clone())
            }
            _ => return None,
        };
        let named = self.wallpaper.len() <= 81 && (css.is_some() || id(&self.wallpaper));
        let within = |value: f64, most: f64| value.is_finite() && (0.0..=most).contains(&value);
        let fits = within(self.padding, 0.3)
            && within(self.corner_radius, 200.0)
            && within(self.shadow_opacity, 1.0);
        (named && fits).then(|| Canvas {
            background: Background::Wallpaper { id: self.wallpaper.clone(), css },
            padding: self.padding,
            corner_radius: self.corner_radius,
            shadow: Shadow { opacity: self.shadow_opacity, ..Shadow::default() },
            ..Canvas::default()
        })
    }
}

/// Takes a screenshot of `source` and saves it as a new project, returning its folder. It starts with `look`, the
/// last screenshot's style, when there is one. For a picture of a whole screen the record dialog steps aside first,
/// and comes back if it fails.
#[tauri::command]
pub async fn take_screenshot<R: Runtime>(
    app: AppHandle<R>,
    source: String,
    folder: Option<String>,
    look: Option<Look>,
) -> Result<String, String> {
    let target = parse_source(&source)?;
    let look = look.as_ref().and_then(Look::canvas);
    let folder = projects_folder(&app, folder.as_deref())?;
    let dialog = app.get_webview_window("record").filter(|_| matches!(target, Target::Display(_)));
    if let Some(dialog) = &dialog {
        let _ = dialog.hide();
    }
    let step_aside = dialog.is_some();
    let taken = tauri::async_runtime::spawn_blocking(move || {
        if step_aside {
            std::thread::sleep(STEP_ASIDE);
        }
        let shot = shotmaxxing_capture::screenshot(target).map_err(|e| e.to_string())?;
        shotmaxxing_capture::save_screenshot(&folder, "Screenshot", target, &shot, look.as_ref())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|taken| taken);
    match taken {
        Ok(finished) => Ok(finished.root.to_string_lossy().into_owned()),
        Err(error) => {
            if let Some(dialog) = &dialog {
                let _ = dialog.show();
                let _ = dialog.set_focus();
            }
            Err(error)
        }
    }
}

/// Copies a screenshot project to the clipboard, styled the way the editor shows it, pixel for pixel.
#[tauri::command]
pub async fn copy_screenshot(path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let still = shotmaxxing_render::draw_screenshot(Path::new(&path), None).map_err(|e| e.to_string())?;
        shotmaxxing_capture::copy_image(&still.png, &still.rgba, still.width, still.height)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look(wallpaper: &str, padding: f64) -> Look {
        Look {
            wallpaper: wallpaper.into(),
            wallpaper_css: None,
            padding,
            corner_radius: 20.0,
            shadow_opacity: 0.3,
        }
    }

    #[test]
    fn a_remembered_look_carries_over_only_when_the_editor_could_have_made_it() {
        let canvas = look("ocean", 0.12).canvas().unwrap();
        assert_eq!(canvas.background, Background::Wallpaper { id: "ocean".into(), css: None });
        assert_eq!((canvas.padding, canvas.corner_radius, canvas.shadow.opacity), (0.12, 20.0, 0.3));
        assert_eq!(canvas.shadow.blur, Shadow::default().blur);
        for bad in [look("", 0.1), look("../Pictures/x", 0.1), look("ocean", 0.9), look("ocean", f64::NAN)] {
            assert!(bad.canvas().is_none(), "{bad:?} should be left out");
        }
    }

    #[test]
    fn a_content_packs_wallpaper_carries_over_with_its_css() {
        let css = "linear-gradient(135deg, oklch(0.82 0.12 75), oklch(0.55 0.17 35))";
        let pack =
            |id: &str, css: Option<&str>| Look { wallpaper_css: css.map(String::from), ..look(id, 0.1) };
        let canvas = pack("northwind.sunrise", Some(css)).canvas().unwrap();
        assert_eq!(
            canvas.background,
            Background::Wallpaper { id: "northwind.sunrise".into(), css: Some(css.into()) }
        );
        // Without its CSS, with CSS that doesn't check out, or with CSS on a built-in name, it's left out.
        for bad in [
            pack("northwind.sunrise", None),
            pack("northwind.sunrise", Some("url(//example.com/a.png)")),
            pack("ocean", Some(css)),
            pack("north wind.sunrise", Some(css)),
        ] {
            assert!(bad.canvas().is_none(), "{bad:?} should be left out");
        }
    }
}
