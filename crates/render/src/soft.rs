//! The portable compositor: the frames the macOS exporter draws with Core Image, drawn on the CPU with tiny-skia,
//! for systems without a GPU path of their own yet. It reads the same scene ([`crate::scene::frame_at`]) and the
//! same sprites, so its frames match the editor and the Mac's. Core Image blends in linear light and this blends in
//! sRGB, so soft edges and blurs can differ by a shade.
//!
//! Each frame draws, in order: the background (moving with the camera), the recording in its rounded window, blurs
//! and arrows inside the window, the click ring, spotlight and cursor, then titles, the keystroke and the camera
//! bubble, which stay put.

use std::collections::HashMap;

use tiny_skia::{
    BlendMode, Color, FillRule, FilterQuality, Mask, Paint, Pattern, Pixmap, PixmapPaint, PixmapRef, Rect,
    SpreadMode, Transform,
};

use crate::RenderError;
use crate::model::{Bubble, RenderProject};
use crate::scene::{Frame, Layout};
use crate::sprites::{self, LINE_HEIGHT, Label, Palette, Sprite, blur_alpha, rounded_rect};
use crate::wallpaper;

/// Draws `text` in the system's semibold interface font, `size` pixels, centred in a `line_height` × `size` tall
/// box, in `color` on transparent.
pub type DrawText = fn(&str, f64, f64, Color) -> Option<Pixmap>;

/// A rectangle of the frame: x, y, width and height in pixels.
type Area = (f64, f64, f64, f64);

pub struct Compositor<'a> {
    project: &'a RenderProject,
    layout: Layout,
    palette: Palette,
    text: DrawText,
    background: Pixmap,
    cursor: Option<Sprite>,
    ring: Option<Sprite>,
    spotlight: Option<Sprite>,
    arrows: Vec<Option<Sprite>>,
    titles: Vec<Option<Label>>,
    keys: HashMap<String, Option<Label>>,
    out: Pixmap,
    /// The window's shape, which clips blurs and arrows, kept while the camera holds still.
    window: Option<(Area, f64, Mask)>,
    screen: Halves,
    camera: Halves,
}

impl<'a> Compositor<'a> {
    pub fn new(project: &'a RenderProject, layout: Layout, text: DrawText) -> Result<Self, RenderError> {
        let palette = Palette::get();
        let (w, h) = (layout.width as u32, layout.height as u32);
        let failed = |what: &str| RenderError::System(format!("couldn't draw {what}"));
        let css = wallpaper::find(&project.wallpaper)
            .map(|w| w.css)
            .ok_or_else(|| RenderError::Project("there are no wallpapers".into()))?;
        let pixels = wallpaper::rasterize(&css, w, h).map_err(RenderError::Project)?;
        let background =
            sprites::background(&layout, pixels, &palette).ok_or_else(|| failed("the background"))?;
        // Drawn at the largest zoom, so they're never enlarged.
        let res = project.max_zoom().clamp(1.0, 4.0);
        let arrows = project
            .arrows
            .iter()
            .map(|a| {
                let (from, to) = (layout.point_to_frame(a.from), layout.point_to_frame(a.to));
                sprites::arrow(&layout, (from.x, from.y), (to.x, to.y), res, &palette)
            })
            .collect();
        let titles = project
            .titles
            .iter()
            .map(|t| {
                let glyphs = text(&t.text, layout.text_px, LINE_HEIGHT, palette.accent_ink)?;
                sprites::title(&layout, &glyphs, t.at, &palette)
            })
            .collect();
        Ok(Self {
            project,
            layout,
            cursor: sprites::cursor(project.cursor_look, layout.cursor_px, res, &palette),
            ring: sprites::ring(layout.cursor_px, res, &palette),
            spotlight: sprites::spotlight(layout.cursor_px, res, &palette),
            palette,
            text,
            background,
            arrows,
            titles,
            keys: HashMap::new(),
            out: Pixmap::new(w, h).ok_or_else(|| failed("a frame that size"))?,
            window: None,
            screen: Halves::default(),
            camera: Halves::default(),
        })
    }

    /// One finished frame: the recording's picture `screen` (and the camera's, when the bubble shows) with everything
    /// the project adds at this moment. Pictures are opaque RGBA.
    pub fn compose(&mut self, frame: &Frame, screen: PixmapRef, camera: Option<PixmapRef>) -> &Pixmap {
        let (project, l, v) = (self.project, self.layout, frame.view);
        let s = v.scale;
        // A rectangle of the camera layer (frame pixels) where it lands in the output.
        let cam = |x: f64, y: f64, w: f64, h: f64| (v.tx + s * x, v.ty + s * y, s * w, s * h);

        if s == 1.0 && v.tx == 0.0 && v.ty == 0.0 {
            self.out.data_mut().copy_from_slice(self.background.data());
        } else {
            self.out.fill(Color::BLACK);
            let paint = PixmapPaint {
                quality: FilterQuality::Bilinear,
                blend_mode: BlendMode::Source,
                ..PixmapPaint::default()
            };
            let t = Transform::from_row(s as f32, 0.0, 0.0, s as f32, v.tx as f32, v.ty as f32);
            self.out.draw_pixmap(0, 0, self.background.as_ref(), &paint, t, None);
        }

        // The recording, in its rounded window.
        let win = l.window;
        let area = cam(win.x, win.y, win.width, win.height);
        let radius = l.radius * s;
        let picture = self.screen.fit(screen, area.2);
        fill(&mut self.out, picture, area, area, radius, None);

        // Blurs and arrows are part of the window, so its corners clip them, as in the editor.
        let clipped = !frame.blurs.is_empty() || frame.arrows.iter().any(|&i| self.arrows[i].is_some());
        let mask = if clipped { window_mask(&mut self.window, &self.out, area, radius) } else { None };
        if let Some(mask) = mask {
            for &i in &frame.blurs {
                let blur = &project.blurs[i];
                let r = l.to_frame(blur.rect);
                let rect = cam(r.x, r.y, r.width, r.height);
                let sigma = l.blur_sigma * blur.strength * s;
                blur_area(&mut self.out, picture, area, rect, l.blur_radius * s, sigma, mask);
            }
            for &i in &frame.arrows {
                if let Some(arrow) = &self.arrows[i] {
                    let (w, h) = size(arrow);
                    let at = cam(arrow.x, arrow.y, w, h);
                    draw(&mut self.out, arrow.pixmap.as_ref(), at, 1.0, BlendMode::SourceOver, Some(mask));
                }
            }
        }

        if let Some(c) = frame.cursor {
            let (hx, hy) = (v.tx + s * c.at.x, v.ty + s * c.at.y);
            let f = s * c.press;
            let at = |p: &Sprite, k: f64| {
                let (w, h) = size(p);
                (hx + p.x * f * k, hy + p.y * f * k, w * f * k, h * f * k)
            };
            if let (Some(age), Some(ring)) = (c.ripple, &self.ring) {
                let opacity = (1.0 - age) as f32;
                draw(
                    &mut self.out,
                    ring.pixmap.as_ref(),
                    at(ring, 0.35 + age * 0.9),
                    opacity,
                    BlendMode::SourceOver,
                    None,
                );
            }
            if project.spotlight
                && let Some(spot) = &self.spotlight
            {
                draw(&mut self.out, spot.pixmap.as_ref(), at(spot, 1.0), 1.0, BlendMode::SourceOver, None);
            }
            if let Some(cursor) = &self.cursor {
                let place = at(cursor, 1.0);
                match c.from {
                    Some(from) => {
                        let back = (v.tx + s * from.x - hx, v.ty + s * from.y - hy);
                        smear(&mut self.out, cursor.pixmap.as_ref(), place, back);
                    }
                    None => {
                        draw(&mut self.out, cursor.pixmap.as_ref(), place, 1.0, BlendMode::SourceOver, None)
                    }
                }
            }
        }

        for &i in &frame.titles {
            if let Some(title) = &self.titles[i] {
                draw_label(&mut self.out, title);
            }
        }
        if let Some(i) = frame.key {
            let label = &project.keys[i].1;
            if !self.keys.contains_key(label) {
                let key = (self.text)(label, l.text_px, LINE_HEIGHT, self.palette.on_stage)
                    .and_then(|glyphs| sprites::key(&l, &glyphs, &self.palette));
                self.keys.insert(label.clone(), key);
            }
            if let Some(Some(key)) = self.keys.get(label) {
                draw_label(&mut self.out, key);
            }
        }
        if let (Some(bubble), Some(camera)) = (&project.bubble, camera) {
            self.bubble(bubble, camera, s);
        }
        &self.out
    }

    /// The camera bubble in its corner, over everything else, sized, ringed and shaded as in the editor: a ring of
    /// 0.4% of the frame height, a shadow 0.8% down with a 1.2% blur, 4% from the edges, and 70% of its size at
    /// full zoom when it shrinks.
    fn bubble(&mut self, bubble: &Bubble, camera: PixmapRef, zoom: f64) {
        let l = self.layout;
        let shrink = if bubble.shrink { 1.0 - 0.3 * ((zoom - 1.0) / 0.5).clamp(0.0, 1.0) } else { 1.0 };
        let d = l.height * bubble.size * shrink;
        let margin = l.height * 0.04;
        let x = if bubble.left { margin } else { l.width - d - margin };
        let y = if bubble.top { margin } else { l.height - d - margin };
        let border = l.height * 0.004;
        let radius = d * bubble.radius;
        if bubble.shadow {
            shadow(&mut self.out, (x, y + l.height * 0.008, d, d), radius, l.height * 0.012, 0.35);
        }
        if let Some(ring) = rounded_rect(x as f32, y as f32, d as f32, d as f32, radius as f32) {
            let mut paint = Paint { anti_alias: true, ..Paint::default() };
            paint.set_color(Color::WHITE);
            self.out.fill_path(&ring, &paint, FillRule::Winding, Transform::identity(), None);
        }
        // The middle square of the camera's picture, like `object-fit: cover`.
        let inner = d - 2.0 * border;
        let (cw, ch) = (f64::from(camera.width()), f64::from(camera.height()));
        let side = cw.min(ch);
        // Where the whole picture lands, so its middle square fills the bubble.
        let k = inner / side;
        let (px, py) = (x + border - (cw - side) / 2.0 * k, y + border - (ch - side) / 2.0 * k);
        let picture = self.camera.fit(camera, cw * k);
        let shape = (x + border, y + border, inner, inner);
        let corner = (radius - border).max(0.0);
        if bubble.mirror {
            let flipped = Transform::from_row(-1.0, 0.0, 0.0, 1.0, (2.0 * x + d) as f32, 0.0);
            fill_transformed(&mut self.out, picture, (px, py, cw * k, ch * k), shape, corner, flipped);
        } else {
            fill(&mut self.out, picture, (px, py, cw * k, ch * k), shape, corner, None);
        }
    }
}

/// A sprite's size in the frame at zoom 1.
fn size(sprite: &Sprite) -> (f64, f64) {
    (f64::from(sprite.pixmap.width()) / sprite.scale, f64::from(sprite.pixmap.height()) / sprite.scale)
}

/// Maps `image`'s pixels onto `area`.
fn stretch(image: PixmapRef, (x, y, w, h): Area) -> Transform {
    let (sx, sy) = (w / f64::from(image.width()), h / f64::from(image.height()));
    Transform::from_row(sx as f32, 0.0, 0.0, sy as f32, x as f32, y as f32)
}

/// Draws `image` stretched over `area`.
fn draw(
    out: &mut Pixmap,
    image: PixmapRef,
    area: Area,
    opacity: f32,
    blend_mode: BlendMode,
    mask: Option<&Mask>,
) {
    if opacity <= 0.0 || area.2 <= 0.0 || area.3 <= 0.0 {
        return;
    }
    let paint = PixmapPaint { opacity: opacity.min(1.0), blend_mode, quality: FilterQuality::Bicubic };
    out.draw_pixmap(0, 0, image, &paint, stretch(image, area), mask);
}

/// A title or keystroke, pixel for pixel.
fn draw_label(out: &mut Pixmap, label: &Label) {
    let paint = PixmapPaint::default();
    out.draw_pixmap(
        label.x as i32,
        label.y as i32,
        label.pixmap.as_ref(),
        &paint,
        Transform::identity(),
        None,
    );
}

/// Fills the rounded rectangle `shape` with `picture` stretched over `area`, its edge pixels repeated outwards, so
/// the shape alone decides where the picture ends.
fn fill(out: &mut Pixmap, picture: PixmapRef, area: Area, shape: Area, radius: f64, mask: Option<&Mask>) {
    let Some(path) =
        rounded_rect(shape.0 as f32, shape.1 as f32, shape.2 as f32, shape.3 as f32, radius as f32)
    else {
        return;
    };
    let shader = Pattern::new(picture, SpreadMode::Pad, FilterQuality::Bilinear, 1.0, stretch(picture, area));
    let paint = Paint { shader, anti_alias: true, ..Paint::default() };
    out.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), mask);
}

/// Like [`fill`], with the picture moved by `transform` after it is stretched (a mirror, for the camera).
fn fill_transformed(
    out: &mut Pixmap,
    picture: PixmapRef,
    area: Area,
    shape: Area,
    radius: f64,
    transform: Transform,
) {
    let Some(path) =
        rounded_rect(shape.0 as f32, shape.1 as f32, shape.2 as f32, shape.3 as f32, radius as f32)
    else {
        return;
    };
    let placed = stretch(picture, area).post_concat(transform);
    let shader = Pattern::new(picture, SpreadMode::Pad, FilterQuality::Bilinear, 1.0, placed);
    let paint = Paint { shader, anti_alias: true, ..Paint::default() };
    out.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
}

/// The window's rounded shape over the whole frame, redrawn only when the camera moved.
fn window_mask<'m>(
    cache: &'m mut Option<(Area, f64, Mask)>,
    out: &Pixmap,
    area: Area,
    radius: f64,
) -> Option<&'m Mask> {
    let current = matches!(cache, Some((a, r, _)) if *a == area && *r == radius);
    if !current {
        let mut mask = Mask::new(out.width(), out.height())?;
        let path = rounded_rect(area.0 as f32, area.1 as f32, area.2 as f32, area.3 as f32, radius as f32)?;
        mask.fill_path(&path, FillRule::Winding, true, Transform::identity());
        *cache = Some((area, radius, mask));
    }
    cache.as_ref().map(|(_, _, mask)| mask)
}

/// Blurs the recording under the rounded rectangle `rect`, inside the window's `mask`. The recording is `picture`
/// stretched over `area`.
fn blur_area(
    out: &mut Pixmap,
    picture: PixmapRef,
    area: Area,
    rect: Area,
    radius: f64,
    sigma: f64,
    mask: &Mask,
) {
    let (fw, fh) = (f64::from(out.width()), f64::from(out.height()));
    // Only the part inside the frame shows.
    let (x0, y0) = (rect.0.max(0.0), rect.1.max(0.0));
    let (x1, y1) = ((rect.0 + rect.2).min(fw), (rect.1 + rect.3).min(fh));
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    // Blurred from a little further out, so what shows blends with what lies beyond it.
    let pad = (3.0 * sigma).ceil() + 2.0;
    let (rx, ry) = ((x0 - pad).floor(), (y0 - pad).floor());
    let (rw, rh) = ((x1 + pad).ceil() - rx, (y1 + pad).ceil() - ry);
    let (Some(mut layer), Some(bounds)) =
        (Pixmap::new(rw as u32, rh as u32), Rect::from_xywh(0.0, 0.0, rw as f32, rh as f32))
    else {
        return;
    };
    let placed = stretch(picture, (area.0 - rx, area.1 - ry, area.2, area.3));
    let shader = Pattern::new(picture, SpreadMode::Pad, FilterQuality::Bilinear, 1.0, placed);
    layer.fill_rect(bounds, &Paint { shader, ..Paint::default() }, Transform::identity(), None);
    blur_rgb(&mut layer, sigma);
    let Some(path) = rounded_rect(rect.0 as f32, rect.1 as f32, rect.2 as f32, rect.3 as f32, radius as f32)
    else {
        return;
    };
    let at = Transform::from_translate(rx as f32, ry as f32);
    let shader = Pattern::new(layer.as_ref(), SpreadMode::Pad, FilterQuality::Nearest, 1.0, at);
    let paint = Paint { shader, anti_alias: true, ..Paint::default() };
    out.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), Some(mask));
}

/// Blurs an opaque picture's colour, one channel at a time.
fn blur_rgb(pixmap: &mut Pixmap, sigma: f64) {
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let mut plane = vec![0f32; w * h];
    for c in 0..3 {
        for (p, px) in plane.iter_mut().zip(pixmap.data().as_chunks::<4>().0) {
            *p = f32::from(px[c]);
        }
        blur_alpha(&mut plane, w, h, sigma);
        for (p, px) in plane.iter().zip(pixmap.data_mut().as_chunks_mut::<4>().0) {
            px[c] = p.round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// The cursor smeared along its path during the shutter: copies from where it was (`back`, an offset from its place
/// now) to where it is, averaged so the smear never gets darker than the cursor, with more weight at the front so it
/// stays easy to follow.
fn smear(out: &mut Pixmap, image: PixmapRef, (x, y, w, h): Area, back: (f64, f64)) {
    let copies = (back.0.hypot(back.1) / (w.max(1.0) * 0.12)).ceil().clamp(2.0, 10.0) as usize;
    let weight = |i: usize| 1.0 + 2.0 * i as f64 / (copies - 1) as f64;
    let total: f64 = (0..copies).map(weight).sum();
    // The copies add up in a layer that covers them all, inside the frame.
    let (fw, fh) = (f64::from(out.width()), f64::from(out.height()));
    let lx = (x + back.0.min(0.0)).floor().max(0.0) - 1.0;
    let ly = (y + back.1.min(0.0)).floor().max(0.0) - 1.0;
    let rx = (x + w + back.0.max(0.0)).ceil().min(fw) + 1.0;
    let ry = (y + h + back.1.max(0.0)).ceil().min(fh) + 1.0;
    if rx <= lx || ry <= ly {
        return;
    }
    let Some(mut layer) = Pixmap::new((rx - lx) as u32, (ry - ly) as u32) else { return };
    for i in 0..copies {
        let behind = 1.0 - i as f64 / (copies - 1) as f64;
        let at = (x + back.0 * behind - lx, y + back.1 * behind - ly, w, h);
        draw(&mut layer, image, at, (weight(i) / total) as f32, BlendMode::Plus, None);
    }
    let paint = PixmapPaint::default();
    out.draw_pixmap(lx as i32, ly as i32, layer.as_ref(), &paint, Transform::identity(), None);
}

/// A soft black shadow of the rounded rectangle `shape`, blurred by `sigma`, at `alpha`.
fn shadow(out: &mut Pixmap, shape: Area, radius: f64, sigma: f64, alpha: f32) {
    let pad = (3.0 * sigma).ceil() + 1.0;
    let (rx, ry) = ((shape.0 - pad).floor(), (shape.1 - pad).floor());
    let (rw, rh) = ((shape.0 + shape.2 + pad).ceil() - rx, (shape.1 + shape.3 + pad).ceil() - ry);
    let (w, h) = (rw as usize, rh as usize);
    let Some(mut layer) = Pixmap::new(rw as u32, rh as u32) else { return };
    let (x, y) = ((shape.0 - rx) as f32, (shape.1 - ry) as f32);
    let Some(path) = rounded_rect(x, y, shape.2 as f32, shape.3 as f32, radius as f32) else { return };
    let mut paint = Paint { anti_alias: true, ..Paint::default() };
    paint.set_color(Color::BLACK);
    layer.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    let mut coverage: Vec<f32> =
        layer.data().as_chunks::<4>().0.iter().map(|p| f32::from(p[3]) / 255.0).collect();
    blur_alpha(&mut coverage, w, h, sigma);
    // Premultiplied black: only the alpha is set.
    for (px, a) in layer.data_mut().as_chunks_mut::<4>().0.iter_mut().zip(coverage) {
        *px = [0, 0, 0, ((a * alpha) * 255.0).round().clamp(0.0, 255.0) as u8];
    }
    let paint = PixmapPaint::default();
    out.draw_pixmap(rx as i32, ry as i32, layer.as_ref(), &paint, Transform::identity(), None);
}

/// Halved copies of a picture, so it is never drawn at less than half its size: filtering then skips pixels and
/// fine text shimmers. Each frame brings a new picture, so the copies are remade as needed, in the same memory.
#[derive(Default)]
struct Halves(Vec<Pixmap>);

impl Halves {
    /// `picture`, or the halved copy closest to `width` pixels wide without going under it.
    fn fit<'p>(&'p mut self, picture: PixmapRef<'p>, width: f64) -> PixmapRef<'p> {
        let mut levels = 0;
        let mut scale = width / f64::from(picture.width());
        while scale <= 0.5 && levels < 4 {
            scale *= 2.0;
            levels += 1;
        }
        if levels == 0 {
            return picture;
        }
        for level in 0..levels {
            let source = if level == 0 { picture } else { self.0[level - 1].as_ref() };
            let (w, h) = (source.width().div_ceil(2), source.height().div_ceil(2));
            if self.0.get(level).is_none_or(|p| p.width() != w || p.height() != h) {
                let Some(fresh) = Pixmap::new(w, h) else { return picture };
                if level < self.0.len() {
                    self.0[level] = fresh;
                } else {
                    self.0.push(fresh);
                }
            }
            let (done, rest) = self.0.split_at_mut(level);
            let source = if level == 0 { picture } else { done[level - 1].as_ref() };
            halve(source, &mut rest[0]);
        }
        self.0[levels - 1].as_ref()
    }
}

/// Averages each 2 × 2 block of `src` into one pixel of `dst`, which is half its size, rounded up.
fn halve(src: PixmapRef, dst: &mut Pixmap) {
    let (sw, sh) = (src.width() as usize, src.height() as usize);
    let dw = dst.width() as usize;
    let s = src.data();
    for (y, row) in dst.data_mut().chunks_exact_mut(dw * 4).enumerate() {
        let r0 = (2 * y).min(sh - 1) * sw * 4;
        let r1 = (2 * y + 1).min(sh - 1) * sw * 4;
        for (x, px) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let c0 = (2 * x).min(sw - 1) * 4;
            let c1 = (2 * x + 1).min(sw - 1) * 4;
            for (c, out) in px.iter_mut().enumerate() {
                let sum = u16::from(s[r0 + c0 + c])
                    + u16::from(s[r0 + c1 + c])
                    + u16::from(s[r1 + c0 + c])
                    + u16::from(s[r1 + c1 + c]);
                *out = ((sum + 2) / 4) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halving_averages_blocks_and_keeps_odd_edges() {
        let mut src = Pixmap::new(3, 2).unwrap();
        for (i, px) in src.data_mut().as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let v = [0, 100, 200, 40, 60, 80][i];
            *px = [v, v, v, 255];
        }
        let mut dst = Pixmap::new(2, 1).unwrap();
        halve(src.as_ref(), &mut dst);
        // (0 + 100 + 40 + 60) / 4 and, at the odd edge, (200 + 200 + 80 + 80) / 4.
        assert_eq!(dst.data(), &[50, 50, 50, 255, 140, 140, 140, 255]);
    }

    #[test]
    fn pictures_are_halved_until_they_are_no_less_than_half_drawn() {
        let picture = Pixmap::new(2880, 1800).unwrap();
        let mut halves = Halves::default();
        assert_eq!(halves.fit(picture.as_ref(), 1600.0).width(), 2880);
        assert_eq!(halves.fit(picture.as_ref(), 1300.0).width(), 1440);
        assert_eq!(halves.fit(picture.as_ref(), 500.0).width(), 720);
        // The copies are reused.
        assert_eq!(halves.0.len(), 2);
        assert_eq!(halves.fit(picture.as_ref(), 1300.0).width(), 1440);
    }

    #[test]
    fn a_blur_keeps_flat_colour_flat() {
        let mut layer = Pixmap::new(40, 30).unwrap();
        layer.fill(Color::from_rgba8(30, 120, 220, 255));
        blur_rgb(&mut layer, 6.0);
        assert!(layer.data().as_chunks::<4>().0.iter().all(|p| *p == [30, 120, 220, 255]));
    }
}
