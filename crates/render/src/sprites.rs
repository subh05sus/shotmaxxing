//! The still parts of a frame, drawn once per export: the background with the window's shadow, the cursor shapes,
//! the click ring and spotlight, arrows, and the pills behind titles and keystrokes. Each frame then only moves and
//! fades them, which is cheap on the GPU.
//!
//! Colours are the editor's tokens (packages/ui/src/styles/tokens.css), converted from OKLCH to sRGB.

use tiny_skia::{
    Color, FillRule, GradientStop, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Point,
    RadialGradient, SpreadMode, Stroke, Transform,
};

use crate::model::CursorLook;
use crate::scene::Layout;

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
    let m = ((12.0 * sigma * sigma - n * (wl * wl) as f64 - 4.0 * n * wl as f64 - 3.0 * n) / (-4.0 * wl as f64 - 4.0))
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
    let mut alpha: Vec<f32> = mask.data().as_chunks::<4>().0.iter().map(|p| f32::from(p[3]) / 255.0).collect();
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

/// The cursor, drawn `res` times larger than its frame size so it stays sharp when the camera zooms in.
pub fn cursor(look: CursorLook, cursor_px: f64, res: f64, palette: &Palette) -> Option<Sprite> {
    let size = cursor_px * res;
    let ring = (cursor_px / 11.0 * res) as f32;
    match look {
        CursorLook::Dot => {
            let d = (size * 0.7) as f32;
            let pad = 2.0;
            let side = (d + pad * 2.0).ceil() as u32;
            let mut pixmap = Pixmap::new(side, side)?;
            let c = side as f32 / 2.0;
            let outer = PathBuilder::from_circle(c, c, d / 2.0)?;
            pixmap.fill_path(&outer, &paint(palette.cursor_light), FillRule::Winding, Transform::identity(), None);
            let inner = PathBuilder::from_circle(c, c, (d / 2.0 - ring).max(0.5))?;
            pixmap.fill_path(&inner, &paint(palette.cursor_dark), FillRule::Winding, Transform::identity(), None);
            Some(Sprite { pixmap, x: -f64::from(c) / res, y: -f64::from(c) / res, scale: res })
        }
        CursorLook::Mac | CursorLook::Windows => {
            // The editor's arrow, in a 20 × 28 box with its tip at (2, 1.5).
            let u = (size / 20.0) as f32;
            let pad = 2.0 * u;
            let (wpx, hpx) = ((20.0 * u + pad * 2.0).ceil() as u32, (28.0 * u + pad * 2.0).ceil() as u32);
            let mut pixmap = Pixmap::new(wpx, hpx)?;
            let mut pb = PathBuilder::new();
            let pts = [(2.0, 1.5), (2.0, 22.0), (7.2, 17.4), (10.6, 25.6), (14.0, 24.2), (10.7, 16.2), (17.6, 16.2)];
            for (i, (x, y)) in pts.iter().enumerate() {
                let (px, py) = (pad + x * u, pad + y * u);
                if i == 0 { pb.move_to(px, py) } else { pb.line_to(px, py) }
            }
            pb.close();
            let path = pb.finish()?;
            let (fill, edge) = if look == CursorLook::Mac {
                (palette.cursor_dark, palette.cursor_light)
            } else {
                (palette.cursor_light, palette.cursor_dark)
            };
            pixmap.fill_path(&path, &paint(fill), FillRule::Winding, Transform::identity(), None);
            let stroke = Stroke { width: 1.6 * u, line_join: LineJoin::Round, ..Stroke::default() };
            pixmap.stroke_path(&path, &paint(edge), &stroke, Transform::identity(), None);
            let (tip_x, tip_y) = (f64::from(pad + 2.0 * u), f64::from(pad + 1.5 * u));
            Some(Sprite { pixmap, x: -tip_x / res, y: -tip_y / res, scale: res })
        }
    }
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
    let mut alpha: Vec<f32> = mask.data().as_chunks::<4>().0.iter().map(|p| f32::from(p[3]) / 255.0).collect();
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
pub fn arrow(layout: &Layout, from: (f64, f64), to: (f64, f64), res: f64, palette: &Palette) -> Option<Sprite> {
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
    let mut pixmap = Pixmap::new(((max_x - min_x) * res).ceil() as u32, ((max_y - min_y) * res).ceil() as u32)?;
    let at = |p: (f64, f64)| (((p.0 - min_x) * res) as f32, ((p.1 - min_y) * res) as f32);
    let mut pb = PathBuilder::new();
    let (s, e, h1, h2) = (at(points[0]), at(points[1]), at(points[2]), at(points[3]));
    pb.move_to(s.0, s.1);
    pb.line_to(e.0, e.1);
    pb.move_to(h1.0, h1.1);
    pb.line_to(e.0, e.1);
    pb.line_to(h2.0, h2.1);
    let path = pb.finish()?;
    let stroke =
        Stroke { width: (width * res) as f32, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
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
        let moved = rounded_rect(margin as f32, (margin + offset) as f32, bw as f32, bh as f32, radius as f32)?;
        mask.fill_path(&moved, &paint(Color::BLACK), FillRule::Winding, Transform::identity(), None);
        let mut alpha: Vec<f32> = mask.data().as_chunks::<4>().0.iter().map(|p| f32::from(p[3]) / 255.0).collect();
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let s = cursor(CursorLook::Mac, 30.0, 2.0, &Palette::get()).unwrap();
        // The tip sits two units of padding plus (2, 1.5) units into the sprite, which is drawn twice as large.
        let u = 60.0 / 20.0;
        assert!((-s.x * 2.0 - (2.0 * u + 2.0 * u)).abs() < 1e-6);
        assert!((-s.y * 2.0 - (2.0 * u + 1.5 * u)).abs() < 1e-6);
    }
}
