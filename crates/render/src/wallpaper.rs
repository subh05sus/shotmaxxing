//! The built-in wallpapers, drawn natively from the same CSS the editor shows.
//!
//! The definitions live in `assets/wallpapers.json`, shared with the editor. They use a small part of CSS: layers of
//! `radial-gradient(<w>% <h>% at <x>% <y>%, …)` and `linear-gradient(<angle>deg, …)` with `oklch()` and
//! `transparent` stops. Like browsers, stops blend in Oklab with premultiplied alpha and layers stack in sRGB, so an
//! export matches the preview. A touch of dither keeps the gradients from banding once the video is compressed.

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Wallpaper {
    pub id: String,
    pub name: String,
    pub css: String,
}

const DATA: &str = include_str!("../assets/wallpapers.json");

/// Every built-in wallpaper.
pub fn wallpapers() -> Vec<Wallpaper> {
    serde_json::from_str(DATA).unwrap_or_default()
}

/// The wallpaper with `id`, or the first one.
pub fn find(id: &str) -> Option<Wallpaper> {
    let all = wallpapers();
    all.iter().find(|w| w.id == id).cloned().or_else(|| all.into_iter().next())
}

/// A colour in Oklab with straight alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Lab {
    l: f64,
    a: f64,
    b: f64,
    alpha: f64,
}

const TRANSPARENT: Lab = Lab { l: 0.0, a: 0.0, b: 0.0, alpha: 0.0 };

#[derive(Clone, Debug, PartialEq)]
struct Stop {
    color: Lab,
    at: f64,
}

#[derive(Clone, Debug, PartialEq)]
enum Shape {
    /// Radii and centre as fractions of the box.
    Radial { rx: f64, ry: f64, cx: f64, cy: f64 },
    /// CSS angle in degrees: 0 points up, 90 to the right.
    Linear { angle: f64 },
}

#[derive(Clone, Debug, PartialEq)]
struct Layer {
    shape: Shape,
    stops: Vec<Stop>,
}

/// Splits on `sep` outside parentheses.
fn split_top(text: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            c if c == sep && depth == 0 => {
                out.push(text[start..i].trim());
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(text[start..].trim());
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

fn percent(token: &str) -> Result<f64, String> {
    token
        .strip_suffix('%')
        .and_then(|n| n.trim().parse::<f64>().ok())
        .map(|n| n / 100.0)
        .ok_or_else(|| format!("expected a percentage, found “{token}”"))
}

fn oklch(inner: &str) -> Result<Lab, String> {
    let (lch, alpha) = match inner.split_once('/') {
        Some((lch, alpha)) => (lch, alpha.trim()),
        None => (inner, "1"),
    };
    let numbers: Vec<f64> = lch
        .split_whitespace()
        .map(|n| match n.strip_suffix('%') {
            Some(p) => p.parse::<f64>().map(|v| v / 100.0),
            None => n.parse::<f64>(),
        })
        .collect::<Result<_, _>>()
        .map_err(|_| format!("couldn't read oklch({inner})"))?;
    let [l, c, h] = numbers[..] else { return Err(format!("couldn't read oklch({inner})")) };
    let alpha = match alpha.strip_suffix('%') {
        Some(p) => p.parse::<f64>().map(|v| v / 100.0),
        None => alpha.parse::<f64>(),
    }
    .map_err(|_| format!("couldn't read the alpha in oklch({inner})"))?;
    let hue = h.to_radians();
    Ok(Lab { l, a: c * hue.cos(), b: c * hue.sin(), alpha: alpha.clamp(0.0, 1.0) })
}

/// `#rrggbb` in Oklab. Only the app's own solid colours and gradients use it: [`check`] keeps it out of packs.
fn hex(token: &str) -> Option<Lab> {
    let digits = token.strip_prefix('#').filter(|d| d.len() == 6)?;
    let channel = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok().map(|v| f64::from(v) / 255.0);
    let [r, g, b] = [channel(0)?, channel(2)?, channel(4)?]
        .map(|v| if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) });
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    Some(Lab {
        l: 0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s,
        a: 1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s,
        b: 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s,
        alpha: 1.0,
    })
}

fn color(token: &str) -> Result<Lab, String> {
    if token == "transparent" {
        return Ok(TRANSPARENT);
    }
    if token.starts_with('#') {
        return hex(token).ok_or_else(|| format!("unsupported colour “{token}”"));
    }
    token
        .strip_prefix("oklch(")
        .and_then(|rest| rest.strip_suffix(')'))
        .ok_or_else(|| format!("unsupported colour “{token}”"))
        .and_then(oklch)
}

fn stops(items: &[&str]) -> Result<Vec<Stop>, String> {
    let mut parsed: Vec<(Lab, Option<f64>)> = Vec::new();
    for item in items {
        // The colour may contain spaces (oklch(…)); the position, if any, is the part after its closing parenthesis.
        let (color_text, position) = match item.rfind(')') {
            Some(end) => (&item[..=end], item[end + 1..].trim()),
            None => match item.split_once(char::is_whitespace) {
                Some((c, p)) => (c, p.trim()),
                None => (*item, ""),
            },
        };
        let at = if position.is_empty() { None } else { Some(percent(position)?) };
        parsed.push((color(color_text.trim())?, at));
    }
    if parsed.len() < 2 {
        return Err("a gradient needs at least two colours".into());
    }
    // CSS fills in missing positions: first 0%, last 100%, the rest spread evenly, never going backwards.
    let n = parsed.len();
    if parsed[0].1.is_none() {
        parsed[0].1 = Some(0.0);
    }
    if parsed[n - 1].1.is_none() {
        parsed[n - 1].1 = Some(1.0);
    }
    let mut i = 1;
    while i < n {
        if parsed[i].1.is_none() {
            let start = i - 1;
            let mut end = i;
            while parsed[end].1.is_none() {
                end += 1;
            }
            let (a, b) = (parsed[start].1.unwrap_or(0.0), parsed[end].1.unwrap_or(1.0));
            for (k, item) in parsed.iter_mut().enumerate().take(end).skip(i) {
                item.1 = Some(a + (b - a) * (k - start) as f64 / (end - start) as f64);
            }
            i = end;
        }
        i += 1;
    }
    let mut last = f64::NEG_INFINITY;
    Ok(parsed
        .into_iter()
        .map(|(color, at)| {
            let at = at.unwrap_or(0.0).max(last);
            last = at;
            Stop { color, at }
        })
        .collect())
}

fn layer(text: &str) -> Result<Layer, String> {
    let (kind, body) = text
        .split_once('(')
        .and_then(|(kind, rest)| rest.strip_suffix(')').map(|body| (kind.trim(), body)))
        .ok_or_else(|| format!("couldn't read “{text}”"))?;
    let parts = split_top(body, ',');
    match kind {
        "radial-gradient" => {
            let head: Vec<&str> = parts.first().map(|h| h.split_whitespace().collect()).unwrap_or_default();
            let [w, h, "at", x, y] = head[..] else {
                return Err(format!("unsupported radial gradient “{text}”"));
            };
            Ok(Layer {
                shape: Shape::Radial { rx: percent(w)?, ry: percent(h)?, cx: percent(x)?, cy: percent(y)? },
                stops: stops(&parts[1..])?,
            })
        }
        "linear-gradient" => {
            let (angle, rest) = match parts.first().and_then(|p| p.strip_suffix("deg")) {
                Some(deg) => {
                    (deg.trim().parse::<f64>().map_err(|_| format!("bad angle in “{text}”"))?, &parts[1..])
                }
                None => (180.0, &parts[..]),
            };
            Ok(Layer { shape: Shape::Linear { angle }, stops: stops(rest)? })
        }
        other => Err(format!("unsupported background “{other}”")),
    }
}

fn parse(css: &str) -> Result<Vec<Layer>, String> {
    split_top(css, ',').into_iter().map(layer).collect()
}

/// The most layers a wallpaper from outside the app (a content pack) may stack.
pub const MOST_LAYERS: usize = 8;

/// Whether `css`, a wallpaper from outside the app, is one the exporter draws exactly as the editor shows it: only
/// the gradients and `oklch()` colours above, at most [`MOST_LAYERS`] of them, covering the whole picture. Nothing
/// else parses, so a wallpaper can't make the editor fetch anything (no `url()`), and with no quotes, colons or
/// semicolons it can't reach past its own `background` either.
pub fn check(css: &str) -> Result<(), String> {
    if css.len() > 4000 {
        return Err("is longer than 4,000 characters".into());
    }
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || " .,%()/-\n\t".contains(c);
    if let Some(c) = css.chars().find(|&c| !allowed(c)) {
        return Err(format!("has “{c}”, which a wallpaper doesn't use: only gradients of oklch() colours"));
    }
    let layers = parse(css).map_err(|e| format!("can't be drawn: {e}"))?;
    if layers.len() > MOST_LAYERS {
        return Err(format!("stacks {} gradients; {MOST_LAYERS} at most", layers.len()));
    }
    let pixels = rasterize(css, 32, 18).map_err(|e| format!("can't be drawn: {e}"))?;
    if pixels.chunks(4).any(|p| p[3] != 255) {
        return Err(
            "leaves part of the picture see-through: end it with a linear-gradient of solid colours".into()
        );
    }
    Ok(())
}

fn lab_to_srgb(c: Lab) -> [f64; 3] {
    let l_ = c.l + 0.396_337_777_4 * c.a + 0.215_803_757_3 * c.b;
    let m_ = c.l - 0.105_561_345_8 * c.a - 0.063_854_172_8 * c.b;
    let s_ = c.l - 0.089_484_177_5 * c.a - 1.291_485_548 * c.b;
    let (l, m, s) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    let linear = [
        4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
        -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
        -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701 * s,
    ];
    linear.map(|v| {
        let v = v.clamp(0.0, 1.0);
        if v <= 0.003_130_8 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
    })
}

/// The gradient's colour at `t` along its line, as premultiplied sRGB.
fn sample(stops: &[Stop], t: f64) -> [f64; 4] {
    let lab = if t <= stops[0].at {
        stops[0].color
    } else if t >= stops[stops.len() - 1].at {
        stops[stops.len() - 1].color
    } else {
        let i = stops.partition_point(|s| s.at <= t).max(1) - 1;
        let (a, b) = (&stops[i], &stops[(i + 1).min(stops.len() - 1)]);
        let span = b.at - a.at;
        let u = if span <= 0.0 { 1.0 } else { (t - a.at) / span };
        // Premultiplied interpolation, so fading to transparent doesn't pass through grey.
        let (pa, pb) = (a.color.alpha, b.color.alpha);
        let alpha = pa + (pb - pa) * u;
        if alpha <= 0.0 {
            TRANSPARENT
        } else {
            let mix = |x: f64, y: f64| (x * pa + (y * pb - x * pa) * u) / alpha;
            Lab {
                l: mix(a.color.l, b.color.l),
                a: mix(a.color.a, b.color.a),
                b: mix(a.color.b, b.color.b),
                alpha,
            }
        }
    };
    let [r, g, b] = lab_to_srgb(lab);
    [r * lab.alpha, g * lab.alpha, b * lab.alpha, lab.alpha]
}

const LUT: usize = 2048;

/// Draws `css` at `width` × `height` into premultiplied RGBA bytes.
pub fn rasterize(css: &str, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let layers = parse(css)?;
    let (w, h) = (width as usize, height as usize);
    let mut acc = vec![[0f32; 4]; w * h];
    // Bottom layer first; each layer above composites over what is below.
    for layer in layers.iter().rev() {
        // Colours depend only on the position along the gradient, so a lookup table does the colour maths once.
        let table: Vec<[f32; 4]> =
            (0..=LUT).map(|i| sample(&layer.stops, i as f64 / LUT as f64 * 1.5).map(|v| v as f32)).collect();
        let lookup = |t: f64| -> [f32; 4] {
            let x = (t.clamp(0.0, 1.5) / 1.5 * LUT as f64) as f32;
            let i = (x as usize).min(LUT - 1);
            let f = x - i as f32;
            let (a, b) = (table[i], table[i + 1]);
            [
                a[0] + (b[0] - a[0]) * f,
                a[1] + (b[1] - a[1]) * f,
                a[2] + (b[2] - a[2]) * f,
                a[3] + (b[3] - a[3]) * f,
            ]
        };
        for y in 0..h {
            let py = y as f64 + 0.5;
            for x in 0..w {
                let px = x as f64 + 0.5;
                let t = match layer.shape {
                    Shape::Radial { rx, ry, cx, cy } => {
                        let dx = (px - cx * w as f64) / (rx * w as f64).max(1e-6);
                        let dy = (py - cy * h as f64) / (ry * h as f64).max(1e-6);
                        (dx * dx + dy * dy).sqrt()
                    }
                    Shape::Linear { angle } => {
                        let (sin, cos) = angle.to_radians().sin_cos();
                        let length = (w as f64 * sin).abs() + (h as f64 * cos).abs();
                        ((px - w as f64 / 2.0) * sin - (py - h as f64 / 2.0) * cos) / length.max(1e-6) + 0.5
                    }
                };
                let src = lookup(t);
                let dst = &mut acc[y * w + x];
                let keep = 1.0 - src[3];
                for c in 0..4 {
                    dst[c] = src[c] + dst[c] * keep;
                }
            }
        }
    }
    let mut out = vec![0u8; w * h * 4];
    for (i, px) in acc.iter().enumerate() {
        // Up to half a step of noise breaks up bands that compression would otherwise make visible.
        let noise = (hash(i as u32) as f32 / u32::MAX as f32 - 0.5) * 0.9;
        for c in 0..4 {
            let v = if c == 3 { px[3] * 255.0 } else { px[c] * 255.0 + noise };
            out[i * 4 + c] = v.round().clamp(0.0, 255.0) as u8;
        }
        // Premultiplied colour can't exceed alpha.
        for c in 0..3 {
            out[i * 4 + c] = out[i * 4 + c].min(out[i * 4 + 3]);
        }
    }
    Ok(out)
}

fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_wallpaper_parses() {
        let all = wallpapers();
        assert_eq!(all.len(), 9);
        for w in &all {
            parse(&w.css).unwrap_or_else(|e| panic!("{}: {e}", w.id));
        }
    }

    #[test]
    fn stop_positions_fill_in_like_css() {
        let layers =
            parse("linear-gradient(135deg, oklch(0.3 0.05 150), oklch(0.5 0 0), oklch(0.18 0.03 200))")
                .unwrap();
        let at: Vec<f64> = layers[0].stops.iter().map(|s| s.at).collect();
        assert_eq!(at, vec![0.0, 0.5, 1.0]);
        let layers =
            parse("radial-gradient(120% 90% at 15% 10%, oklch(0.88 0.2 128) 0%, transparent 55%)").unwrap();
        assert_eq!(layers[0].shape, Shape::Radial { rx: 1.2, ry: 0.9, cx: 0.15, cy: 0.1 });
        assert_eq!(layers[0].stops[1].at, 0.55);
        assert_eq!(layers[0].stops[1].color.alpha, 0.0);
    }

    #[test]
    fn white_and_black_convert_exactly() {
        let white = lab_to_srgb(Lab { l: 1.0, a: 0.0, b: 0.0, alpha: 1.0 });
        let black = lab_to_srgb(Lab { l: 0.0, a: 0.0, b: 0.0, alpha: 1.0 });
        for v in white {
            assert!((v - 1.0).abs() < 1e-3, "{white:?}");
        }
        assert_eq!(black, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn rasterizes_opaque_wallpapers() {
        for w in wallpapers() {
            let pixels = rasterize(&w.css, 64, 36).unwrap();
            assert_eq!(pixels.len(), 64 * 36 * 4);
            assert!(pixels.chunks(4).all(|p| p[3] == 255), "{} has see-through pixels", w.id);
        }
        // Graphite is dark at the bottom right and lighter top left.
        let g = rasterize(&find("graphite").unwrap().css, 64, 36).unwrap();
        let luma = |i: usize| g[i * 4] as u32 + g[i * 4 + 1] as u32 + g[i * 4 + 2] as u32;
        assert!(luma(64 * 2 + 12) > luma(64 * 35 + 63));
    }
}
