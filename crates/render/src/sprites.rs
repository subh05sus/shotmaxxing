//! The still parts of a frame, drawn once per export: the background with the window's shadow, the cursor shapes,
//! the click ring and spotlight, arrows, and the pills behind titles and keystrokes. Each frame then only moves and
//! fades them. Both compositors use them: Core Image on macOS and the portable one elsewhere.
//!
//! Colours are the editor's tokens (packages/ui/src/styles/tokens.css), converted from OKLCH to sRGB.

use tiny_skia::{
    Color, FillRule, GradientStop, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Point,
    RadialGradient, SpreadMode, Stroke, Transform,
};

use shotmaxxing_core::project::CaptionStyle;

use crate::model::CursorLook;
use crate::scene::Layout;

/// The line box of titles, captions and the keystroke overlay, as a multiple of the font size.
pub const LINE_HEIGHT: f64 = 1.25;

/// How heavy a label's letters are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    /// Bar captions, as `font-medium` in the editor.
    Medium,
    /// Titles and keystrokes, as `font-semibold`.
    Semibold,
    /// Bold captions, as `font-extrabold`.
    Heavy,
}

/// Converts an OKLCH colour to straight-alpha sRGB.
pub fn oklch(l: f64, c: f64, h: f64, alpha: f64) -> Color {
    let hue = h.to_radians();
    let (a, b) = (c * hue.cos(), c * hue.sin());
    let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = l - 0.089_484_177_5 * a - 1.291_485_548 * b;
    let (lc, mc, sc) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    let encode = |v: f64| {
        let v = v.clamp(0.0, 1.0);
        (if v <= 0.003_130_8 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }) as f32
    };
    let r = encode(4.076_741_662_1 * lc - 3.307_711_591_3 * mc + 0.230_969_929_2 * sc);
    let g = encode(-1.268_438_004_6 * lc + 2.609_757_401_1 * mc - 0.341_319_396_5 * sc);
    let bl = encode(-0.004_196_086_3 * lc - 0.703_418_614_7 * mc + 1.707_614_701 * sc);
    Color::from_rgba(r, g, bl, alpha as f32).unwrap_or(Color::BLACK)
}

pub struct Palette {
    pub cursor_light: Color,
    pub cursor_dark: Color,
    /// The soft dark edge around the click ring.
    pub cursor_shadow: Color,
    pub accent: Color,
    pub accent_ink: Color,
    pub scrim: Color,
    pub on_stage: Color,
    pub shadow_ink: Color,
    /// Captions: darker than the keystroke scrim, with solid letters, so they read on any picture.
    pub caption_scrim: Color,
    pub caption_ink: Color,
}

impl Palette {
    pub fn get() -> Self {
        Self {
            cursor_light: oklch(1.0, 0.0, 0.0, 1.0),
            cursor_dark: oklch(0.16, 0.0, 0.0, 1.0),
            cursor_shadow: oklch(0.0, 0.0, 0.0, 0.45),
            accent: oklch(0.87, 0.21, 128.0, 1.0),
            accent_ink: oklch(0.22, 0.05, 130.0, 1.0),
            scrim: oklch(0.0, 0.0, 0.0, 0.5),
            on_stage: oklch(0.97, 0.0, 0.0, 0.86),
            shadow_ink: oklch(0.12, 0.01, 264.0, 1.0),
            caption_scrim: oklch(0.0, 0.0, 0.0, 0.75),
            caption_ink: oklch(0.98, 0.0, 0.0, 1.0),
        }
    }
}

/// A drawn picture and where it goes. `x`, `y` is its top-left corner in frame pixels: relative to the cursor's
/// hotspot for the cursor's parts, in the camera layer for arrows. `scale` is how many sprite pixels make one frame
/// pixel, so the sprite stays sharp when the camera zooms in.
pub struct Sprite {
    pub pixmap: Pixmap,
    pub x: f64,
    pub y: f64,
    pub scale: f64,
}

fn paint(color: Color) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(color);
    paint.anti_alias = true;
    paint
}

/// A rounded rectangle path.
pub fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    // Cubic Béziers with this handle length trace a quarter circle closely.
    let k = r * 0.552_284_8;
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

/// Approximates a Gaussian blur of an alpha buffer with three box blurs, each across then down.
pub fn blur_alpha(buf: &mut [f32], w: usize, h: usize, sigma: f64) {
    if sigma < 0.5 || w == 0 || h == 0 {
        return;
    }
    // Box sizes whose three passes match the Gaussian's spread.
    let n = 3.0;
    let ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = ideal.floor() as i64;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wu = wl + 2;
    let m = ((12.0 * sigma * sigma - n * (wl * wl) as f64 - 4.0 * n * wl as f64 - 3.0 * n)
        / (-4.0 * wl as f64 - 4.0))
        .round() as i64;
    let mut tmp = vec![0f32; buf.len()];
    for pass in 0..3 {
        let size = if pass < m { wl } else { wu };
        let r = ((size - 1) / 2).max(0) as usize;
        box_pass(buf, &mut tmp, w, h, r, true);
        box_pass(&tmp, buf, w, h, r, false);
    }
}

fn box_pass(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize, across: bool) {
    let (lines, len, step, stride) = if across { (h, w, 1, w) } else { (w, h, w, 1) };
    let scale = 1.0 / (2 * r + 1) as f32;
    for line in 0..lines {
        let base = line * stride;
        let at = |i: isize| -> f32 {
            let i = i.clamp(0, len as isize - 1) as usize;
            src[base + i * step]
        };
        let mut sum: f32 = (-(r as isize)..=r as isize).map(at).sum();
        for i in 0..len {
            dst[base + i * step] = sum * scale;
            sum += at(i as isize + r as isize + 1) - at(i as isize - r as isize);
        }
    }
}

/// The background: wallpaper with the window's shadow on it, at frame size.
pub fn background(layout: &Layout, wallpaper: Vec<u8>, palette: &Palette) -> Option<Pixmap> {
    let (w, h) = (layout.width as u32, layout.height as u32);
    let mut pixmap = Pixmap::from_vec(wallpaper, tiny_skia::IntSize::from_wh(w, h)?)?;
    if layout.shadow_alpha <= 0.0 {
        return Some(pixmap);
    }
    // The shadow is the window's shape, moved down and blurred.
    let mut mask = Pixmap::new(w, h)?;
    let win = layout.window;
    let path = rounded_rect(
        win.x as f32,
        (win.y + layout.shadow_offset) as f32,
        win.width as f32,
        win.height as f32,
        layout.radius as f32,
    )?;
    mask.fill_path(&path, &paint(Color::WHITE), FillRule::Winding, Transform::identity(), None);
    let mut alpha: Vec<f32> =
        mask.data().as_chunks::<4>().0.iter().map(|p| f32::from(p[3]) / 255.0).collect();
    blur_alpha(&mut alpha, w as usize, h as usize, layout.shadow_sigma);
    let ink = palette.shadow_ink;
    let strength = layout.shadow_alpha as f32;
    let (r, g, b) = (ink.red(), ink.green(), ink.blue());
    for (px, a) in pixmap.data_mut().as_chunks_mut::<4>().0.iter_mut().zip(alpha) {
        let a = (a * strength).clamp(0.0, 1.0);
        if a <= 0.0 {
            continue;
        }
        // Premultiplied source-over with the shadow colour.
        for (c, v) in [r, g, b].into_iter().enumerate() {
            let dst = f32::from(px[c]) / 255.0;
            px[c] = ((v * a + dst * (1.0 - a)) * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        let dst_a = f32::from(px[3]) / 255.0;
        px[3] = ((a + dst_a * (1.0 - a)) * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    Some(pixmap)
}

/// One part of a cursor's drawing, in the units of its 20-wide box (`assets/cursors.json`, which the editor draws too).
#[derive(Clone, Debug, serde::Deserialize)]
struct CursorLayer {
    /// An SVG path of absolute `M`, `L`, `C`, `Q` and `Z` commands.
    #[serde(default)]
    d: Option<String>,
    /// A circle: centre x, centre y, radius.
    #[serde(default)]
    circle: Option<[f32; 3]>,
    /// `light` or `dark`, the cursor tokens; none leaves it unfilled.
    #[serde(default)]
    fill: Option<String>,
    #[serde(default)]
    stroke: Option<String>,
    #[serde(default)]
    width: f32,
    #[serde(default = "one")]
    opacity: f32,
}

fn one() -> f32 {
    1.0
}

#[derive(Clone, Debug, serde::Deserialize)]
struct CursorShape {
    id: String,
    hotspot: [f32; 2],
    layers: Vec<CursorLayer>,
}

const CURSORS: &str = include_str!("../assets/cursors.json");

fn shapes() -> Vec<CursorShape> {
    serde_json::from_str(CURSORS).unwrap_or_default()
}

/// The id of `look` in `assets/cursors.json`.
fn shape_id(look: CursorLook) -> &'static str {
    match look {
        CursorLook::Mac => "mac",
        CursorLook::MacWhite => "mac-white",
        CursorLook::Windows => "windows",
        CursorLook::Outline => "outline",
        CursorLook::Dot => "dot",
        CursorLook::Ring => "ring",
        CursorLook::Bold => "bold",
        CursorLook::Hand => "hand",
    }
}

/// Reads an SVG path's absolute `M`, `L`, `C`, `Q` and `Z` commands, with its points and the path.
fn svg_path(d: &str) -> Option<(Vec<(f32, f32)>, Path)> {
    let mut tokens = Vec::new();
    let mut number = String::new();
    let flush = |number: &mut String, tokens: &mut Vec<String>| {
        if !number.is_empty() {
            tokens.push(std::mem::take(number));
        }
    };
    for c in d.chars() {
        if c.is_ascii_alphabetic() {
            flush(&mut number, &mut tokens);
            tokens.push(c.to_string());
        } else if c == ',' || c.is_whitespace() {
            flush(&mut number, &mut tokens);
        } else if c == '-' && !number.is_empty() && !number.ends_with('e') {
            flush(&mut number, &mut tokens);
            number.push(c);
        } else {
            number.push(c);
        }
    }
    flush(&mut number, &mut tokens);
    let mut pb = PathBuilder::new();
    let mut points = Vec::new();
    let mut i = 0;
    let mut command = ' ';
    let take = |i: &mut usize, n: usize| -> Option<Vec<f32>> {
        let values: Option<Vec<f32>> = tokens.get(*i..*i + n)?.iter().map(|t| t.parse().ok()).collect();
        *i += n;
        values
    };
    while i < tokens.len() {
        if let Some(c) = tokens[i].chars().next().filter(char::is_ascii_alphabetic) {
            command = c;
            i += 1;
            if command == 'Z' || command == 'z' {
                pb.close();
                continue;
            }
        }
        match command {
            'M' => {
                let v = take(&mut i, 2)?;
                pb.move_to(v[0], v[1]);
                points.push((v[0], v[1]));
                command = 'L';
            }
            'L' => {
                let v = take(&mut i, 2)?;
                pb.line_to(v[0], v[1]);
                points.push((v[0], v[1]));
            }
            'Q' => {
                let v = take(&mut i, 4)?;
                pb.quad_to(v[0], v[1], v[2], v[3]);
                points.extend([(v[0], v[1]), (v[2], v[3])]);
            }
            'C' => {
                let v = take(&mut i, 6)?;
                pb.cubic_to(v[0], v[1], v[2], v[3], v[4], v[5]);
                points.extend([(v[0], v[1]), (v[2], v[3]), (v[4], v[5])]);
            }
            _ => return None,
        }
    }
    Some((points, pb.finish()?))
}

/// The most the arrow leans into its movement, in degrees, and the steps it's drawn at.
pub const SWAY_MAX_DEG: f64 = 25.0;
pub const SWAY_STEP_DEG: f64 = 2.5;

/// Which of the tilted cursors [`cursors`] drew is the one for `angle` degrees.
pub fn sway_index(angle: f64, count: usize) -> usize {
    if count <= 1 {
        return 0;
    }
    let half = (count - 1) / 2;
    let steps = (angle / SWAY_STEP_DEG).round().clamp(-(half as f64), half as f64) as i64;
    (half as i64 + steps) as usize
}

/// The cursor tilted from `-SWAY_MAX_DEG` to `SWAY_MAX_DEG` degrees in `SWAY_STEP_DEG` steps when it sways, or
/// upright only. Pick one with [`sway_index`].
pub fn cursors(look: CursorLook, cursor_px: f64, res: f64, sway: bool, palette: &Palette) -> Vec<Sprite> {
    if !sway {
        return cursor(look, cursor_px, res, 0.0, palette).into_iter().collect();
    }
    let half = (SWAY_MAX_DEG / SWAY_STEP_DEG).round() as i32;
    (-half..=half)
        .filter_map(|i| cursor(look, cursor_px, res, f64::from(i) * SWAY_STEP_DEG, palette))
        .collect()
}

/// The cursor, drawn `res` times larger than its frame size so it stays sharp when the camera zooms in, turned
/// `angle` degrees clockwise about its hotspot. The sprite is square and centred on the hotspot, so turning it never
/// moves the tip.
pub fn cursor(look: CursorLook, cursor_px: f64, res: f64, angle: f64, palette: &Palette) -> Option<Sprite> {
    let shape = shapes().into_iter().find(|s| s.id == shape_id(look))?;
    let u = (cursor_px * res / 20.0) as f32;
    let [hx, hy] = shape.hotspot;
    let mut parts = Vec::new();
    let mut reach = 0f32;
    for layer in &shape.layers {
        let (points, path) = match (&layer.d, layer.circle) {
            (Some(d), _) => svg_path(d)?,
            (None, Some([cx, cy, r])) => {
                let reach = vec![(cx - r, cy - r), (cx + r, cy + r), (cx - r, cy + r), (cx + r, cy - r)];
                (reach, PathBuilder::from_circle(cx, cy, r)?)
            }
            _ => continue,
        };
        for (x, y) in points {
            reach = reach.max(((x - hx).powi(2) + (y - hy).powi(2)).sqrt() + layer.width / 2.0);
        }
        parts.push((layer, path));
    }
    // A unit of room around the furthest point, so antialiased edges aren't cut.
    let side = (((reach + 1.0) * 2.0 * u).ceil() as u32).max(2);
    let mut pixmap = Pixmap::new(side, side)?;
    let c = side as f32 / 2.0;
    let transform =
        Transform::from_translate(c, c).pre_rotate(angle as f32).pre_scale(u, u).pre_translate(-hx, -hy);
    let token = |name: &Option<String>, opacity: f32| {
        let color = match name.as_deref()? {
            "light" => palette.cursor_light,
            "dark" => palette.cursor_dark,
            _ => return None,
        };
        let mut color = color;
        color.apply_opacity(opacity);
        Some(color)
    };
    for (layer, path) in &parts {
        if let Some(fill) = token(&layer.fill, layer.opacity) {
            pixmap.fill_path(path, &paint(fill), FillRule::Winding, transform, None);
        }
        if let Some(edge) = token(&layer.stroke, layer.opacity)
            && layer.width > 0.0
        {
            let stroke = Stroke { width: layer.width, line_join: LineJoin::Round, ..Stroke::default() };
            pixmap.stroke_path(path, &paint(edge), &stroke, transform, None);
        }
    }
    let at = -f64::from(c) / res;
    Some(Sprite { pixmap, x: at, y: at, scale: res })
}

/// The ring a click leaves: `2 × cursor_px` across at full size, centred on the hotspot, with a soft dark edge
/// (CSS `drop-shadow(0 0 0.6 × ring)`) so it shows on light pages too.
pub fn ring(cursor_px: f64, res: f64, palette: &Palette) -> Option<Sprite> {
    let d = (cursor_px * 2.0 * res) as f32;
    let border = (cursor_px / 11.0 * res) as f32;
    // A drop-shadow's blur radius is twice the Gaussian's standard deviation.
    let sigma = f64::from(border) * 0.3;
    let margin = (sigma * 3.0).ceil() as f32 + 2.0;
    let side = (d + margin * 2.0).ceil() as u32;
    let c = side as f32 / 2.0;
    let path = PathBuilder::from_circle(c, c, d / 2.0 - border / 2.0)?;
    let stroke = Stroke { width: border, ..Stroke::default() };

    let mut mask = Pixmap::new(side, side)?;
    mask.stroke_path(&path, &paint(Color::WHITE), &stroke, Transform::identity(), None);
    let mut alpha: Vec<f32> =
        mask.data().as_chunks::<4>().0.iter().map(|p| f32::from(p[3]) / 255.0).collect();
    blur_alpha(&mut alpha, side as usize, side as usize, sigma);
    let mut pixmap = Pixmap::new(side, side)?;
    let shadow = palette.cursor_shadow;
    for (px, a) in pixmap.data_mut().as_chunks_mut::<4>().0.iter_mut().zip(alpha) {
        let a = (a * shadow.alpha()).clamp(0.0, 1.0);
        let premultiplied = [shadow.red() * a, shadow.green() * a, shadow.blue() * a, a];
        for (out, v) in px.iter_mut().zip(premultiplied) {
            *out = (v * 255.0).round() as u8;
        }
    }
    pixmap.stroke_path(&path, &paint(palette.cursor_light), &stroke, Transform::identity(), None);
    Some(Sprite { pixmap, x: -f64::from(c) / res, y: -f64::from(c) / res, scale: res })
}

/// The soft light around the cursor.
pub fn spotlight(cursor_px: f64, res: f64, palette: &Palette) -> Option<Sprite> {
    let radius = (cursor_px * 3.0 * res) as f32;
    let side = (radius * 2.0).ceil() as u32;
    let mut pixmap = Pixmap::new(side, side)?;
    let c = side as f32 / 2.0;
    let light = palette.cursor_light;
    let (r, g, b) = (light.red(), light.green(), light.blue());
    // CSS's default size is the farthest corner; the box is a circle, so the glow fades out just inside it.
    let reach = radius * std::f32::consts::SQRT_2;
    let shader = RadialGradient::new(
        Point::from_xy(c, c),
        Point::from_xy(c, c),
        reach,
        vec![
            GradientStop::new(0.0, Color::from_rgba(r, g, b, 0.28)?),
            GradientStop::new(0.7, Color::from_rgba(r, g, b, 0.0)?),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )?;
    let fill = Paint { shader, anti_alias: true, ..Paint::default() };
    let circle = PathBuilder::from_circle(c, c, radius)?;
    pixmap.fill_path(&circle, &fill, FillRule::Winding, Transform::identity(), None);
    Some(Sprite { pixmap, x: -f64::from(c) / res, y: -f64::from(c) / res, scale: res })
}

/// An arrow from `from` to `to` (frame pixels in the camera layer), drawn `res` times larger.
pub fn arrow(
    layout: &Layout,
    from: (f64, f64),
    to: (f64, f64),
    res: f64,
    palette: &Palette,
) -> Option<Sprite> {
    let k = layout.k;
    let width = f64::max(3.0, 7.0 * k);
    let head = f64::max(10.0, 26.0 * k);
    let angle = (to.1 - from.1).atan2(to.0 - from.0);
    let (a1, a2) = (angle + std::f64::consts::PI - 0.5, angle + std::f64::consts::PI + 0.5);
    let points = [
        from,
        to,
        (to.0 + head * a1.cos(), to.1 + head * a1.sin()),
        (to.0 + head * a2.cos(), to.1 + head * a2.sin()),
    ];
    let pad = width;
    let min_x = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min) - pad;
    let min_y = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min) - pad;
    let max_x = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max) + pad;
    let max_y = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max) + pad;
    let mut pixmap =
        Pixmap::new(((max_x - min_x) * res).ceil() as u32, ((max_y - min_y) * res).ceil() as u32)?;
    let at = |p: (f64, f64)| (((p.0 - min_x) * res) as f32, ((p.1 - min_y) * res) as f32);
    let mut pb = PathBuilder::new();
    let (s, e, h1, h2) = (at(points[0]), at(points[1]), at(points[2]), at(points[3]));
    pb.move_to(s.0, s.1);
    pb.line_to(e.0, e.1);
    pb.move_to(h1.0, h1.1);
    pb.line_to(e.0, e.1);
    pb.line_to(h2.0, h2.1);
    let path = pb.finish()?;
    let stroke = Stroke {
        width: (width * res) as f32,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    pixmap.stroke_path(&path, &paint(palette.accent), &stroke, Transform::identity(), None);
    Some(Sprite { pixmap, x: min_x, y: min_y, scale: res })
}

/// A pill behind `text` (already drawn, premultiplied, at frame scale), with `pad_x` and `pad_y` around it.
pub fn label(
    text: &Pixmap,
    pad_x: f64,
    pad_y: f64,
    radius: f64,
    fill: Color,
    shadow: Option<(f64, f64, f32)>,
) -> Option<Pixmap> {
    let (tw, th) = (f64::from(text.width()), f64::from(text.height()));
    let (bw, bh) = (tw + pad_x * 2.0, th + pad_y * 2.0);
    // Room for the shadow below and around the pill.
    let margin = shadow.map_or(0.0, |(offset, sigma, _)| offset + sigma * 3.0).ceil();
    let (w, h) = ((bw + margin * 2.0).ceil() as u32, (bh + margin * 2.0).ceil() as u32);
    let mut pixmap = Pixmap::new(w, h)?;
    let body = rounded_rect(margin as f32, margin as f32, bw as f32, bh as f32, radius as f32)?;
    if let Some((offset, sigma, strength)) = shadow {
        let mut mask = Pixmap::new(w, h)?;
        let moved =
            rounded_rect(margin as f32, (margin + offset) as f32, bw as f32, bh as f32, radius as f32)?;
        mask.fill_path(&moved, &paint(Color::BLACK), FillRule::Winding, Transform::identity(), None);
        let mut alpha: Vec<f32> =
            mask.data().as_chunks::<4>().0.iter().map(|p| f32::from(p[3]) / 255.0).collect();
        blur_alpha(&mut alpha, w as usize, h as usize, sigma);
        for (px, a) in pixmap.data_mut().as_chunks_mut::<4>().0.iter_mut().zip(alpha) {
            px[3] = ((a * strength) * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    pixmap.fill_path(&body, &paint(fill), FillRule::Winding, Transform::identity(), None);
    pixmap.draw_pixmap(
        (margin + pad_x).round() as i32,
        (margin + pad_y).round() as i32,
        text.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    Some(pixmap)
}

/// How far a label's pixmap extends beyond the pill on each side.
pub fn label_margin(shadow: Option<(f64, f64, f32)>) -> f64 {
    shadow.map_or(0.0, |(offset, sigma, _)| offset + sigma * 3.0).ceil()
}

/// A title or keystroke, drawn at frame scale with its top-left corner on whole pixels, above the camera.
pub struct Label {
    pub pixmap: Pixmap,
    pub x: f64,
    pub y: f64,
}

/// A title: a lime pill with a soft shadow behind `glyphs` (drawn in the accent's ink), its top-left corner at `at`
/// (fractions of the frame).
pub fn title(
    layout: &Layout,
    glyphs: &Pixmap,
    at: shotmaxxing_core::geometry::Point,
    palette: &Palette,
) -> Option<Label> {
    let size = layout.text_px;
    let shadow = Some((0.25 * size, 0.375 * size, 0.3));
    let pill = f64::from(glyphs.height()) + size;
    let pixmap = label(glyphs, 1.4 * size, 0.5 * size, pill / 2.0, palette.accent, shadow)?;
    let margin = label_margin(shadow);
    let x = (at.x * layout.width - margin).round();
    let y = (at.y * layout.height - margin).round();
    Some(Label { pixmap, x, y })
}

/// A caption's font size in pixels.
pub fn caption_px(layout: &Layout, style: CaptionStyle) -> f64 {
    layout.height * style.size()
}

/// How a caption's letters are drawn.
pub fn caption_weight(style: CaptionStyle) -> Weight {
    match style {
        CaptionStyle::Bar => Weight::Medium,
        CaptionStyle::Bold => Weight::Heavy,
    }
}

/// `text` (light letters) with a dark edge round its letters and a soft shadow under them, so it reads on any
/// picture. The editor draws the same with stacked CSS text shadows: three of `0 0 0.08em` black at 90% for the edge
/// and `0 0.05em 0.24em` at 60% for the shadow. Returns the picture and the room it has around `text`.
fn outlined(text: &Pixmap, size: f64) -> Option<(Pixmap, usize)> {
    // Offset down, blur, strength and opacity, the lengths as fractions of the font size. A strength above one makes
    // a blur's soft fringe a solid edge, as stacked shadows do.
    let layers = [(0.05 * size, 0.12 * size, 1.0, 0.6), (0.0, 0.04 * size, 3.0, 0.9)];
    let margin =
        layers.iter().map(|(offset, sigma, ..)| offset + 3.0 * sigma).fold(0.0, f64::max).ceil() as usize;
    let (tw, th) = (text.width() as usize, text.height() as usize);
    let (w, h) = (tw + 2 * margin, th + 2 * margin);
    let source: Vec<f32> = text.data().as_chunks::<4>().0.iter().map(|p| f32::from(p[3]) / 255.0).collect();
    let mut shadow = vec![0f32; w * h];
    for (offset, sigma, strength, opacity) in layers {
        let dy = offset.round() as usize;
        let mut alpha = vec![0f32; w * h];
        for y in 0..th {
            let row = (y + margin + dy) * w + margin;
            alpha[row..row + tw].copy_from_slice(&source[y * tw..(y + 1) * tw]);
        }
        blur_alpha(&mut alpha, w, h, sigma);
        for (s, a) in shadow.iter_mut().zip(alpha) {
            let a = (a * strength).clamp(0.0, 1.0) * opacity;
            *s = 1.0 - (1.0 - *s) * (1.0 - a);
        }
    }
    let mut pixmap = Pixmap::new(w as u32, h as u32)?;
    // Black, premultiplied: only the alpha.
    for (px, s) in pixmap.data_mut().as_chunks_mut::<4>().0.iter_mut().zip(shadow) {
        *px = [0, 0, 0, (s.clamp(0.0, 1.0) * 255.0).round() as u8];
    }
    pixmap.draw_pixmap(
        margin as i32,
        margin as i32,
        text.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    Some((pixmap, margin))
}

/// A caption as a compositor keeps it between frames: which one, its pages, and the page drawn (with where its top
/// is), so each page is drawn once.
pub struct CaptionShown<T> {
    index: usize,
    pages: Vec<Vec<String>>,
    page: usize,
    pub drawn: Option<(T, f64)>,
}

impl<T> CaptionShown<T> {
    /// What shows of caption `index` (`text`, in rows of `max` units) `fraction` of the way through its time: what
    /// `last` drew when that's the same page, or else the page's rows drawn by `draw`.
    pub fn next(
        last: Option<Self>,
        (index, fraction): (usize, f64),
        text: &str,
        max: usize,
        draw: impl FnOnce(&[String]) -> Option<(T, f64)>,
    ) -> Self {
        let pages = match last {
            Some(last) if last.index == index => {
                if shotmaxxing_core::captions::page_at(&last.pages, fraction) == last.page {
                    return last;
                }
                last.pages
            }
            _ => shotmaxxing_core::captions::pages(text, max),
        };
        let page = shotmaxxing_core::captions::page_at(&pages, fraction);
        let drawn = pages.get(page).and_then(|rows| draw(rows));
        Self { index, pages, page, drawn }
    }
}

/// A caption: its `rows` (each drawn at frame scale in the caption's ink) stacked and centred, the block's foot 5%
/// above the frame's. A bar caption sits on a dark rounded bar (`px-[0.7em] py-[0.25em] rounded-[0.4em]` in the
/// editor); a bold one has a dark edge round its letters instead. Also returns where what shows begins, so a
/// keystroke can sit above it.
pub fn caption(
    layout: &Layout,
    rows: &[Pixmap],
    style: CaptionStyle,
    palette: &Palette,
) -> Option<(Label, f64)> {
    let size = caption_px(layout, style);
    let width = rows.iter().map(Pixmap::width).max()?;
    let height: u32 = rows.iter().map(Pixmap::height).sum();
    let mut block = Pixmap::new(width, height)?;
    let mut y = 0;
    for row in rows {
        let x = ((width - row.width()) / 2) as i32;
        block.draw_pixmap(x, y, row.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
        y += row.height() as i32;
    }
    let (pixmap, margin) = match style {
        CaptionStyle::Bar => {
            (label(&block, 0.7 * size, 0.25 * size, 0.4 * size, palette.caption_scrim, None)?, 0.0)
        }
        CaptionStyle::Bold => {
            let (pixmap, margin) = outlined(&block, size)?;
            (pixmap, margin as f64)
        }
    };
    let (w, h) = (f64::from(pixmap.width()), f64::from(pixmap.height()));
    let x = ((layout.width - w) / 2.0).round();
    let y = (layout.height * 0.95 - (h - margin)).round();
    Some((Label { pixmap, x, y }, y + margin))
}

/// Where a keystroke goes while a caption shows: above the caption's `top`, with half a line between, when its
/// usual place would overlap it.
pub fn key_above(key: &Label, top: f64, layout: &Layout) -> f64 {
    key.y.min((top - 0.5 * layout.text_px - f64::from(key.pixmap.height())).round())
}

/// A keystroke: `glyphs` (drawn in the stage's ink) centred near the bottom on a dark scrim.
pub fn key(layout: &Layout, glyphs: &Pixmap, palette: &Palette) -> Option<Label> {
    let size = layout.text_px;
    let pixmap = label(glyphs, 0.8 * size, 0.35 * size, 0.5 * size, palette.scrim, None)?;
    let (w, h) = (f64::from(pixmap.width()), f64::from(pixmap.height()));
    let x = ((layout.width - w) / 2.0).round();
    let y = (layout.height * 0.94 - h).round();
    Some(Label { pixmap, x, y })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> Layout {
        let project = crate::RenderProject::from_parts(
            std::path::Path::new("/tmp"),
            &shotmaxxing_core::project::Project::default(),
            &[],
            &[],
        );
        Layout::new(&project, 1920, 1080)
    }

    fn glyphs(w: u32, h: u32) -> Pixmap {
        let mut pixmap = Pixmap::new(w, h).unwrap();
        pixmap.fill(Color::WHITE);
        pixmap
    }

    #[test]
    fn a_bar_caption_sits_centred_above_the_foot_and_a_keystroke_above_it() {
        let (l, palette) = (layout(), Palette::get());
        let size = caption_px(&l, CaptionStyle::Bar);
        let (label, top) =
            caption(&l, &[glyphs(600, 51), glyphs(400, 51)], CaptionStyle::Bar, &palette).unwrap();
        let (w, h) = (f64::from(label.pixmap.width()), f64::from(label.pixmap.height()));
        assert!((w - (600.0 + 1.4 * size)).abs() <= 1.0, "the widest row and its padding: {w}");
        assert!((label.x + w / 2.0 - 960.0).abs() <= 1.0, "centred");
        assert!((label.y + h - 1026.0).abs() <= 1.0, "5% above the foot: {}", label.y + h);
        assert_eq!(top, label.y);

        let key = Label { pixmap: glyphs(80, 40), x: 920.0, y: (1080.0 * 0.94 - 40.0_f64).round() };
        let y = key_above(&key, top, &l);
        assert!(y + 40.0 <= top - 0.4 * l.text_px, "clear of the caption: {y}");
        let far = Label { pixmap: glyphs(80, 40), x: 920.0, y: 100.0 };
        assert_eq!(key_above(&far, top, &l), 100.0, "a keystroke already clear stays put");
    }

    #[test]
    fn a_bold_caption_keeps_its_letters_above_the_foot_with_a_shadow_around_them() {
        let (l, palette) = (layout(), Palette::get());
        let (label, top) = caption(&l, &[glyphs(500, 68)], CaptionStyle::Bold, &palette).unwrap();
        let margin = top - label.y;
        assert!(margin > 0.0, "room for the shadow");
        assert!((top + 68.0 - 1026.0).abs() <= 1.0, "the letters end 5% above the foot: {}", top + 68.0);
        // The shadow is dark and soft below the letters, and clear far from them.
        let below = label.pixmap.pixel(margin as u32 + 250, (margin + 68.0 + 2.0) as u32).unwrap();
        assert!(below.alpha() > 20 && below.red() < 10, "{below:?}");
        let corner = label.pixmap.pixel(0, 0).unwrap();
        assert!(corner.alpha() < 10, "{corner:?}");
    }

    #[test]
    fn blur_keeps_the_total_and_spreads_it() {
        let (w, h) = (41, 41);
        let mut buf = vec![0f32; w * h];
        buf[20 * w + 20] = 1.0;
        blur_alpha(&mut buf, w, h, 3.0);
        let total: f32 = buf.iter().sum();
        assert!((total - 1.0).abs() < 1e-3, "{total}");
        assert!(buf[20 * w + 20] < 0.1 && buf[20 * w + 23] > 0.0);
    }

    #[test]
    fn tokens_convert() {
        let white = oklch(1.0, 0.0, 0.0, 1.0);
        assert!(white.red() > 0.99 && white.green() > 0.99 && white.blue() > 0.99);
        let lime = oklch(0.87, 0.21, 128.0, 1.0);
        assert!(lime.green() > lime.red() && lime.green() > lime.blue());
    }

    #[test]
    fn cursor_tip_is_the_anchor() {
        let palette = Palette::get();
        for look in [
            CursorLook::Mac,
            CursorLook::MacWhite,
            CursorLook::Windows,
            CursorLook::Outline,
            CursorLook::Dot,
            CursorLook::Ring,
            CursorLook::Bold,
            CursorLook::Hand,
        ] {
            let s = cursor(look, 30.0, 2.0, 0.0, &palette).unwrap_or_else(|| panic!("{look:?} doesn't draw"));
            // The hotspot is the sprite's centre, which is drawn twice as large.
            let side = f64::from(s.pixmap.width());
            assert_eq!(s.pixmap.width(), s.pixmap.height());
            assert!((-s.x * 2.0 - side / 2.0).abs() < 1e-6 && (-s.y * 2.0 - side / 2.0).abs() < 1e-6);
            // Something is drawn, and it stays inside the sprite.
            let alpha = |x: u32, y: u32| s.pixmap.pixel(x, y).map_or(0, |p| p.alpha());
            let n = s.pixmap.width();
            assert!((0..n).any(|i| alpha(i, n / 2 + 2) > 0 || alpha(n / 2 + 2, i) > 0), "{look:?} is empty");
            assert!((0..n).all(|i| alpha(i, 0) == 0 && alpha(0, i) == 0), "{look:?} is clipped");
            if let Ok(keep) = std::env::var("SHOTMAXXING_KEEP_EXPORT") {
                let big = cursor(look, 60.0, 2.0, 0.0, &palette).unwrap();
                big.pixmap
                    .save_png(std::path::Path::new(&keep).join(format!("cursor-{look:?}.png")))
                    .unwrap();
            }
        }
    }

    #[test]
    fn a_swaying_cursor_is_drawn_at_each_tilt() {
        let tilted = cursors(CursorLook::Mac, 30.0, 1.0, true, &Palette::get());
        assert_eq!(tilted.len(), 21);
        assert_eq!(sway_index(0.0, 21), 10);
        assert_eq!((sway_index(-99.0, 21), sway_index(99.0, 21), sway_index(3.0, 21)), (0, 20, 11));
        assert_eq!(cursors(CursorLook::Mac, 30.0, 1.0, false, &Palette::get()).len(), 1);
    }

    #[test]
    fn svg_paths_read_lines_and_curves() {
        let (points, _) = svg_path("M2 1.5 L2 22 C3 4 5 6 7 8 Q1,2 3,4 Z").unwrap();
        assert_eq!(points.len(), 7);
        assert!(svg_path("M2 1.5 A1 1 0 0 1 3 3").is_none());
    }
}
