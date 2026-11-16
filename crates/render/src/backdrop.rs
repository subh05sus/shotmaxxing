//! What fills the frame behind the window: a gradient (the built-in and content-pack wallpapers, a solid colour or a
//! gradient someone made), one of the photos that ship with the app, or a picture someone chose. Any of them can be
//! blurred.
//!
//! Photos and pictures fill the frame the way CSS `background-size: cover` does: scaled until they cover it, centred,
//! with the overflow cropped. The blur is a Gaussian (three box passes) with its edges extended, as the editor's
//! preview draws it with a blurred layer a little larger than the frame.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::wallpaper;

/// A background as the renderer draws it.
#[derive(Clone, Debug, PartialEq)]
pub enum Backdrop {
    /// Gradients in the wallpaper CSS subset ([`wallpaper::rasterize`]).
    Css(String),
    /// A built-in photo, by id.
    Photo(String),
    /// A picture file.
    Image(PathBuf),
}

/// A photo that ships with the app.
#[derive(Clone, Debug, Deserialize)]
pub struct Photo {
    pub id: String,
    pub name: String,
}

const PHOTO_LIST: &str = include_str!("../assets/photos.json");

macro_rules! photo_bytes {
    ($($id:literal),* $(,)?) => {
        &[$(($id, include_bytes!(concat!("../assets/photos/", $id, ".jpg")) as &[u8])),*]
    };
}

/// Each photo's JPEG, 2560 × 1440. The list matches `assets/photos.json` (a test checks).
const PHOTO_BYTES: &[(&str, &[u8])] = photo_bytes![
    "blue-ridges",
    "teal-ridges",
    "rose-ridges",
    "ember-ridges",
    "dusk-ridges",
    "coastline",
    "cobalt",
    "bloom",
    "glass-drop",
    "bubbles",
    "lagoon",
    "emerald",
    "fern",
    "jade-arches",
    "sunset",
    "mocha",
    "silk",
    "lilac-petals",
    "lavender",
    "alpenglow",
    "summit",
    "starry-peak",
    "night-ribbon",
    "glow",
];

/// Every built-in photo, in the order the editor shows them.
pub fn photos() -> Vec<Photo> {
    serde_json::from_str(PHOTO_LIST).unwrap_or_default()
}

/// The JPEG of the built-in photo `id`.
pub fn photo(id: &str) -> Option<&'static [u8]> {
    PHOTO_BYTES.iter().find(|(name, _)| *name == id).map(|(_, bytes)| *bytes)
}

/// The most blur the editor offers, in pixels at 1080p.
pub const MOST_BLUR: f64 = 40.0;

/// Draws `backdrop` at `width` × `height` into premultiplied RGBA bytes, blurred by `blur` (sigma in pixels at
/// 1080p). A picture that can't be read (moved, deleted or not an image) falls back to `fallback`'s gradient, so an
/// export still finishes; the error is logged.
pub fn paint(
    backdrop: &Backdrop,
    blur: f64,
    fallback: &str,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let picture = match backdrop {
        Backdrop::Css(css) => return blurred(wallpaper::rasterize(css, width, height)?, blur, width, height),
        Backdrop::Photo(id) => {
            photo(id).ok_or_else(|| format!("there's no built-in photo “{id}”")).and_then(decode)
        }
        Backdrop::Image(path) => read(path),
    };
    let pixels = match picture {
        Ok(image) => cover(&image, width, height),
        Err(e) => {
            tracing::warn!("the background picture can't be drawn ({e}); using a wallpaper instead");
            wallpaper::rasterize(fallback, width, height)?
        }
    };
    blurred(pixels, blur, width, height)
}

fn read(path: &Path) -> Result<image::RgbaImage, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("couldn't open {}: {e}", path.display()))?;
    decode(&bytes)
}

fn decode(bytes: &[u8]) -> Result<image::RgbaImage, String> {
    let image = image::load_from_memory(bytes).map_err(|e| format!("couldn't read the picture: {e}"))?;
    if image.width() == 0 || image.height() == 0 {
        return Err("the picture is empty".into());
    }
    Ok(image.into_rgba8())
}

/// Scales `image` to cover `width` × `height`, centred, as premultiplied RGBA on black (a see-through picture
/// shows black behind it, as in the editor).
fn cover(image: &image::RgbaImage, width: u32, height: u32) -> Vec<u8> {
    let (iw, ih) = (f64::from(image.width()), f64::from(image.height()));
    let scale = (f64::from(width) / iw).max(f64::from(height) / ih);
    let (cw, ch) = ((f64::from(width) / scale).min(iw), (f64::from(height) / scale).min(ih));
    let (x, y) = (((iw - cw) / 2.0).round() as u32, ((ih - ch) / 2.0).round() as u32);
    let (cw, ch) = ((cw.round() as u32).max(1), (ch.round() as u32).max(1));
    let crop = image::imageops::crop_imm(image, x, y, cw, ch).to_image();
    let scaled = if (cw, ch) == (width, height) {
        crop
    } else {
        image::imageops::resize(&crop, width, height, image::imageops::FilterType::CatmullRom)
    };
    let mut out = scaled.into_raw();
    for px in out.as_chunks_mut::<4>().0 {
        let a = u16::from(px[3]);
        for c in px.iter_mut().take(3) {
            *c = ((u16::from(*c) * a + 127) / 255) as u8;
        }
        px[3] = 255;
    }
    out
}

fn blurred(mut pixels: Vec<u8>, blur: f64, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let sigma = if blur.is_finite() { blur.clamp(0.0, MOST_BLUR) } else { 0.0 } * f64::from(height) / 1080.0;
    if sigma >= 0.3 {
        blur_rgba(&mut pixels, width as usize, height as usize, sigma);
    }
    Ok(pixels)
}

/// The widths of three box blurs that together approximate a Gaussian of `sigma`.
fn boxes(sigma: f64) -> [usize; 3] {
    let ideal = (12.0 * sigma * sigma / 3.0 + 1.0).sqrt();
    let mut lower = ideal.floor() as usize;
    if lower.is_multiple_of(2) {
        lower = lower.saturating_sub(1);
    }
    let lower = lower.max(1);
    let upper = lower + 2;
    let l = lower as f64;
    let m = ((12.0 * sigma * sigma - 3.0 * l * l - 12.0 * l - 9.0) / (-4.0 * l - 4.0)).round();
    let m = m.clamp(0.0, 3.0) as usize;
    std::array::from_fn(|i| if i < m { lower } else { upper })
}

/// Blurs premultiplied RGBA in place with a Gaussian of `sigma` pixels, extending the edges.
pub fn blur_rgba(pixels: &mut [u8], w: usize, h: usize, sigma: f64) {
    if w == 0 || h == 0 || pixels.len() < w * h * 4 {
        return;
    }
    let mut a: Vec<f32> = pixels.iter().map(|&v| f32::from(v)).collect();
    let mut b = vec![0f32; a.len()];
    for width in boxes(sigma) {
        let r = (width - 1) / 2;
        box_pass(&a, &mut b, w, h, r, true);
        box_pass(&b, &mut a, w, h, r, false);
    }
    for (out, v) in pixels.iter_mut().zip(&a) {
        *out = v.round().clamp(0.0, 255.0) as u8;
    }
}

/// One box blur of radius `r` along rows (`across`) or columns, on four interleaved channels, edges extended.
fn box_pass(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize, across: bool) {
    let (lines, len) = if across { (h, w) } else { (w, h) };
    let at = |line: usize, i: usize| if across { (line * w + i) * 4 } else { (i * w + line) * 4 };
    let norm = 1.0 / (2 * r + 1) as f32;
    for line in 0..lines {
        let mut sum = [0f32; 4];
        // The window starts centred on 0, with the first pixel repeated off the edge.
        for k in 0..=(2 * r) {
            let i = k.saturating_sub(r).min(len - 1);
            let p = at(line, i);
            for c in 0..4 {
                sum[c] += src[p + c];
            }
        }
        for i in 0..len {
            let p = at(line, i);
            for c in 0..4 {
                dst[p + c] = sum[c] * norm;
            }
            let add = at(line, (i + r + 1).min(len - 1));
            let drop = at(line, i.saturating_sub(r));
            for c in 0..4 {
                sum[c] += src[add + c] - src[drop + c];
            }
        }
    }
}

/// Whether `text` is a `#rrggbb` colour, the form the editor saves solid colours and gradients in.
pub fn is_hex(text: &str) -> bool {
    text.len() == 7 && text.starts_with('#') && text[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// A solid colour as wallpaper CSS.
pub fn solid_css(color: &str) -> Option<String> {
    is_hex(color).then(|| format!("linear-gradient(180deg, {color}, {color})"))
}

/// A two-colour gradient as wallpaper CSS. The editor draws the same with `linear-gradient(in oklab …)`.
pub fn gradient_css(from: &str, to: &str, angle_deg: f64) -> Option<String> {
    (is_hex(from) && is_hex(to) && angle_deg.is_finite())
        .then(|| format!("linear-gradient({}deg, {from}, {to})", angle_deg.rem_euclid(360.0).round()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_photo_ships_and_decodes() {
        let list = photos();
        assert_eq!(list.len(), PHOTO_BYTES.len());
        for p in &list {
            let bytes = photo(&p.id).unwrap_or_else(|| panic!("{} isn't embedded", p.id));
            let image = decode(bytes).unwrap();
            assert_eq!((image.width(), image.height()), (2560, 1440), "{}", p.id);
        }
    }

    #[test]
    fn a_photo_covers_any_shape_opaquely() {
        for (w, h) in [(64, 36), (36, 64), (50, 50)] {
            let px = paint(&Backdrop::Photo("summit".into()), 0.0, "", w, h).unwrap();
            assert_eq!(px.len(), (w * h * 4) as usize);
            assert!(px.chunks(4).all(|p| p[3] == 255));
        }
    }

    #[test]
    fn a_missing_picture_falls_back_to_the_wallpaper() {
        let css = wallpaper::find("graphite").unwrap().css;
        let px = paint(&Backdrop::Image("/no/such/picture.png".into()), 0.0, &css, 32, 18).unwrap();
        assert_eq!(px, wallpaper::rasterize(&css, 32, 18).unwrap());
    }

    #[test]
    fn blur_keeps_flat_colour_and_softens_an_edge() {
        let (w, h) = (40usize, 10usize);
        let mut flat = vec![200u8; w * h * 4];
        blur_rgba(&mut flat, w, h, 4.0);
        assert!(flat.iter().all(|&v| v == 200));
        // Left half black, right half white: after a blur the middle is grey and the far edges stay put.
        let mut edge: Vec<u8> =
            (0..w * h).flat_map(|i| if i % w < w / 2 { [0, 0, 0, 255] } else { [255; 4] }).collect();
        blur_rgba(&mut edge, w, h, 3.0);
        let at = |x: usize| edge[(5 * w + x) * 4];
        assert!(at(0) < 5 && at(w - 1) > 250);
        assert!((90..170).contains(&at(w / 2)), "{}", at(w / 2));
    }

    #[test]
    fn solid_colours_and_gradients_become_wallpaper_css() {
        let solid = solid_css("#ff8800").unwrap();
        let px = wallpaper::rasterize(&solid, 8, 8).unwrap();
        // The dither moves a channel by at most one step.
        assert!(px.chunks(4).all(|p| p[0] >= 254 && (135..=137).contains(&p[1]) && p[2] <= 1 && p[3] == 255));
        assert!(solid_css("red").is_none() && solid_css("#12345").is_none());
        let g = gradient_css("#000000", "#ffffff", 90.0).unwrap();
        let px = wallpaper::rasterize(&g, 64, 4).unwrap();
        assert!(px[0] < 10 && px[63 * 4] > 245);
        assert_eq!(
            gradient_css("#000000", "#ffffff", -90.0).unwrap(),
            "linear-gradient(270deg, #000000, #ffffff)"
        );
    }
}
