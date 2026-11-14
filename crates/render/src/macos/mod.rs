//! The macOS export. AVFoundation decodes the recording in hardware, Core Image draws each frame on the GPU, and
//! AVFoundation encodes it in hardware. Only the still parts (wallpaper, shadow, cursor, labels) are drawn on the
//! CPU, once per export.
//!
//! Every layer is placed straight into the output frame with the camera applied, rather than drawing the canvas
//! first and zooming it afterwards. A zoom therefore shows the recording's real pixels, not an enlarged copy, and the
//! cursor and arrows, drawn at the largest zoom, stay sharp.

mod decode;
mod text;
mod video;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2_core_foundation::{CFRetained, CGAffineTransform, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGColorSpace, kCGColorSpaceSRGB};
use objc2_core_image::{
    CIColor, CIContext, CIContextOption, CIFilter, CIImage, CIVector, kCIContextPriorityRequestLow, kCIFormatRGBA8,
};
use objc2_core_video::CVPixelBuffer;
use objc2_foundation::{NSData, NSDictionary, NSNumber, NSString, ns_string};
use shotmaxxing_core::geometry::Point;
use tiny_skia::Pixmap;

use self::decode::{Decoder, system};
use self::video::VideoWriter;
use crate::model::RenderProject;
use crate::scene::{Frame, Layout, frame_at};
use crate::sprites::{self, Palette, Sprite};
use crate::{ExportOptions, Format, RenderError, wallpaper};

/// Line height of titles and keystrokes, as in the editor.
const LINE_HEIGHT: f64 = 1.25;

/// A picture ready to place. `x`, `y`, `w`, `h` are in frame pixels; what they are relative to depends on the layer.
struct Placed {
    image: Retained<CIImage>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

fn to_ci(pixmap: &Pixmap, space: &CGColorSpace) -> Retained<CIImage> {
    let data = NSData::with_bytes(pixmap.data());
    let size = CGSize::new(f64::from(pixmap.width()), f64::from(pixmap.height()));
    // SAFETY: the data is premultiplied RGBA of exactly this size, as Core Image expects.
    unsafe {
        CIImage::imageWithBitmapData_bytesPerRow_size_format_colorSpace(
            &data,
            pixmap.width() as usize * 4,
            size,
            kCIFormatRGBA8,
            Some(space),
        )
    }
}

fn placed(sprite: Sprite, space: &CGColorSpace) -> Placed {
    let (w, h) = (f64::from(sprite.pixmap.width()) / sprite.scale, f64::from(sprite.pixmap.height()) / sprite.scale);
    Placed { image: to_ci(&sprite.pixmap, space), x: sprite.x, y: sprite.y, w, h }
}

fn params(pairs: &[(&NSString, &AnyObject)]) -> Retained<NSDictionary<NSString, AnyObject>> {
    let keys: Vec<&NSString> = pairs.iter().map(|(k, _)| *k).collect();
    let values: Vec<&AnyObject> = pairs.iter().map(|(_, v)| *v).collect();
    NSDictionary::from_slices(&keys, &values)
}

struct Compositor<'a> {
    project: &'a RenderProject,
    layout: Layout,
    space: CFRetained<CGColorSpace>,
    palette: Palette,
    white: Retained<CIColor>,
    background: Retained<CIImage>,
    cursor: Option<Placed>,
    ring: Option<Placed>,
    spotlight: Option<Placed>,
    arrows: Vec<Option<Placed>>,
    titles: Vec<Option<Placed>>,
    keys: HashMap<String, Option<Placed>>,
}

impl<'a> Compositor<'a> {
    fn new(project: &'a RenderProject, layout: Layout) -> Result<Self, RenderError> {
        let space = CGColorSpace::with_name(Some(
            // SAFETY: an immutable framework constant.
            unsafe { kCGColorSpaceSRGB },
        ))
        .ok_or_else(|| system("the sRGB colour space is missing"))?;
        let palette = Palette::get();
        let (w, h) = (layout.width as u32, layout.height as u32);
        let css = wallpaper::find(&project.wallpaper).map(|w| w.css).ok_or_else(|| system("no wallpapers"))?;
        let pixels = wallpaper::rasterize(&css, w, h).map_err(RenderError::Project)?;
        let background = sprites::background(&layout, pixels, &palette)
            .ok_or_else(|| system("couldn't draw the background"))?;
        let background = to_ci(&background, &space);
        // Drawn at the largest zoom, so they're never enlarged.
        let res = project.max_zoom().clamp(1.0, 4.0);
        let cursor = sprites::cursor(project.cursor_look, layout.cursor_px, res, &palette).map(|s| placed(s, &space));
        let ring = sprites::ring(layout.cursor_px, res, &palette).map(|s| placed(s, &space));
        let spotlight = sprites::spotlight(layout.cursor_px, res, &palette).map(|s| placed(s, &space));
        let arrows = project
            .arrows
            .iter()
            .map(|a| {
                let (from, to) = (layout.point_to_frame(a.from), layout.point_to_frame(a.to));
                sprites::arrow(&layout, (from.x, from.y), (to.x, to.y), res, &palette).map(|s| placed(s, &space))
            })
            .collect();
        // SAFETY: an immutable framework colour.
        let white = unsafe { CIColor::whiteColor() };
        let mut compositor = Self {
            project,
            layout,
            space,
            palette,
            white,
            background,
            cursor,
            ring,
            spotlight,
            arrows,
            titles: Vec::new(),
            keys: HashMap::new(),
        };
        compositor.titles = project.titles.iter().map(|t| compositor.title(&t.text, t.at)).collect();
        Ok(compositor)
    }

    /// A title: a lime pill with a soft shadow, its top-left corner at `at` (fractions of the frame).
    fn title(&self, text: &str, at: Point) -> Option<Placed> {
        let (size, layout) = (self.layout.text_px, &self.layout);
        let glyphs = text::draw(text, size, LINE_HEIGHT, self.palette.accent_ink)?;
        let shadow = Some((0.25 * size, 0.375 * size, 0.3));
        let pill = f64::from(glyphs.height()) + size;
        let pixmap = sprites::label(&glyphs, 1.4 * size, 0.5 * size, pill / 2.0, self.palette.accent, shadow)?;
        let margin = sprites::label_margin(shadow);
        let (w, h) = (f64::from(pixmap.width()), f64::from(pixmap.height()));
        let x = (at.x * layout.width - margin).round();
        let y = (at.y * layout.height - margin).round();
        Some(Placed { image: to_ci(&pixmap, &self.space), x, y, w, h })
    }

    /// A keystroke: centred near the bottom on a dark scrim.
    fn key(&self, label: &str) -> Option<Placed> {
        let (size, layout) = (self.layout.text_px, &self.layout);
        let glyphs = text::draw(label, size, LINE_HEIGHT, self.palette.on_stage)?;
        let pixmap = sprites::label(&glyphs, 0.8 * size, 0.35 * size, 0.5 * size, self.palette.scrim, None)?;
        let (w, h) = (f64::from(pixmap.width()), f64::from(pixmap.height()));
        let x = ((layout.width - w) / 2.0).round();
        let y = (layout.height * 0.94 - h).round();
        Some(Placed { image: to_ci(&pixmap, &self.space), x, y, w, h })
    }

    /// Stretches `image` over the rectangle `x`, `y`, `w`, `h` of the output frame (y down).
    fn place(&self, image: &CIImage, x: f64, y: f64, w: f64, h: f64) -> Retained<CIImage> {
        let (t, downsample) = self.transform(image, x, y, w, h);
        // SAFETY: transforming an image is a pure operation.
        unsafe { image.imageByApplyingTransform_highQualityDownsample(t, downsample) }
    }

    /// Like `place`, with the edge pixels repeated outwards, so a mask alone decides where the picture ends. Without
    /// this the picture's own soft edge and the mask's soft edge add up to a faint dark line.
    fn place_edgeless(&self, image: &CIImage, x: f64, y: f64, w: f64, h: f64) -> Retained<CIImage> {
        let (t, downsample) = self.transform(image, x, y, w, h);
        // SAFETY: clamping and transforming an image are pure operations.
        unsafe { image.imageByClampingToExtent().imageByApplyingTransform_highQualityDownsample(t, downsample) }
    }

    fn transform(&self, image: &CIImage, x: f64, y: f64, w: f64, h: f64) -> (CGAffineTransform, bool) {
        // SAFETY: reading an image's extent is a pure operation.
        let e = unsafe { image.extent() };
        let (sx, sy) = (w / e.size.width, h / e.size.height);
        let (dx, dy) = (x, self.layout.height - y - h);
        let t = CGAffineTransform { a: sx, b: 0.0, c: 0.0, d: sy, tx: dx - e.origin.x * sx, ty: dy - e.origin.y * sy };
        (t, sx < 1.0 || sy < 1.0)
    }

    /// A rectangle of the output frame (y down) in Core Image's coordinates (y up).
    fn rect(&self, x: f64, y: f64, w: f64, h: f64) -> CGRect {
        CGRect::new(CGPoint::new(x, self.layout.height - y - h), CGSize::new(w, h))
    }

    fn rounded(&self, rect: CGRect, radius: f64) -> Option<Retained<CIImage>> {
        // SAFETY: a built-in generator with valid inputs.
        unsafe {
            let extent = CIVector::vectorWithCGRect(rect);
            let radius = NSNumber::new_f64(radius.max(0.0));
            let p = params(&[
                (ns_string!("inputExtent"), extent.as_ref()),
                (ns_string!("inputRadius"), radius.as_ref()),
                (ns_string!("inputColor"), self.white.as_ref()),
            ]);
            CIFilter::filterWithName_withInputParameters(ns_string!("CIRoundedRectangleGenerator"), Some(&p))?
                .outputImage()
        }
    }

    fn opacity(image: Retained<CIImage>, alpha: f64) -> Retained<CIImage> {
        if alpha >= 0.999 {
            return image;
        }
        // SAFETY: a built-in filter with valid inputs.
        unsafe {
            let vector = CIVector::vectorWithX_Y_Z_W(0.0, 0.0, 0.0, alpha.max(0.0));
            image.imageByApplyingFilter_withInputParameters(
                ns_string!("CIColorMatrix"),
                &params(&[(ns_string!("inputAVector"), vector.as_ref())]),
            )
        }
    }

    fn masked(image: &CIImage, background: &CIImage, mask: &CIImage) -> Retained<CIImage> {
        // SAFETY: a built-in filter with valid inputs.
        unsafe {
            image.imageByApplyingFilter_withInputParameters(
                ns_string!("CIBlendWithAlphaMask"),
                &params(&[
                    (ns_string!("inputBackgroundImage"), background.as_ref()),
                    (ns_string!("inputMaskImage"), mask.as_ref()),
                ]),
            )
        }
    }

    fn over(top: &CIImage, bottom: &CIImage) -> Retained<CIImage> {
        // SAFETY: compositing two images is a pure operation.
        unsafe { top.imageByCompositingOverImage(bottom) }
    }

    /// One finished frame: the recording's picture `screen` with everything the project adds at this moment.
    fn compose(&mut self, frame: &Frame, screen: &CVPixelBuffer) -> Retained<CIImage> {
        let (project, l) = (self.project, self.layout);
        let v = frame.view;
        let s = v.scale;
        // A rectangle of the camera layer (frame pixels, y down) where it lands in the output.
        let cam = |x: f64, y: f64, w: f64, h: f64| (v.tx + s * x, v.ty + s * y, s * w, s * h);

        let (bx, by, bw, bh) = cam(0.0, 0.0, l.width, l.height);
        let background = self.place(&self.background, bx, by, bw, bh);

        // The recording, straight from its own pixels to where the camera puts it.
        // SAFETY: the pixel buffer comes from the decoder and stays alive for this frame.
        let picture = unsafe { CIImage::imageWithCVPixelBuffer(screen) };
        let win = l.window;
        let (wx, wy, ww, wh) = cam(win.x, win.y, win.width, win.height);
        let mut content = self.place_edgeless(&picture, wx, wy, ww, wh);

        for &i in &frame.blurs {
            let blur = &project.blurs[i];
            let r = l.to_frame(blur.rect);
            let (x, y, w, h) = cam(r.x, r.y, r.width, r.height);
            let rect = self.rect(x, y, w, h);
            // Cropped a little wider than the mask, so the mask's edge falls on solid blurred pixels.
            let room = self.rect(x - 2.0, y - 2.0, w + 4.0, h + 4.0);
            // SAFETY: pure Core Image operations on valid images.
            let blurred = unsafe {
                content
                    .imageByApplyingGaussianBlurWithSigma(l.blur_sigma * blur.strength * s)
                    .imageByCroppingToRect(room)
            };
            if let Some(mask) = self.rounded(rect, l.blur_radius * s) {
                content = Self::masked(&blurred, &content, &mask);
            }
        }

        // Arrows are part of the window, so its corners clip them, as in the editor.
        for &i in &frame.arrows {
            if let Some(arrow) = &self.arrows[i] {
                let (x, y, w, h) = cam(arrow.x, arrow.y, arrow.w, arrow.h);
                content = Self::over(&self.place(&arrow.image, x, y, w, h), &content);
            }
        }

        let mut out = match self.rounded(self.rect(wx, wy, ww, wh), l.radius * s) {
            Some(mask) => Self::masked(&content, &background, &mask),
            None => Self::over(&content, &background),
        };

        if let Some(c) = frame.cursor {
            let (hx, hy) = (v.tx + s * c.at.x, v.ty + s * c.at.y);
            let f = s * c.press;
            let at = |p: &Placed, k: f64| {
                let w = p.w * f * k;
                let h = p.h * f * k;
                (hx + p.x * f * k, hy + p.y * f * k, w, h)
            };
            if let (Some(age), Some(ring)) = (c.ripple, &self.ring) {
                let (x, y, w, h) = at(ring, 0.35 + age * 0.9);
                let image = Self::opacity(self.place(&ring.image, x, y, w, h), 1.0 - age);
                out = Self::over(&image, &out);
            }
            if project.spotlight
                && let Some(spot) = &self.spotlight
            {
                let (x, y, w, h) = at(spot, 1.0);
                out = Self::over(&self.place(&spot.image, x, y, w, h), &out);
            }
            if let Some(cursor) = &self.cursor {
                let (x, y, w, h) = at(cursor, 1.0);
                out = Self::over(&self.place(&cursor.image, x, y, w, h), &out);
            }
        }

        // SAFETY: cropping is a pure operation.
        out = unsafe { out.imageByCroppingToRect(self.rect(0.0, 0.0, l.width, l.height)) };

        for &i in &frame.titles {
            if let Some(title) = &self.titles[i] {
                out = Self::over(&self.place(&title.image, title.x, title.y, title.w, title.h), &out);
            }
        }
        if let Some(i) = frame.key {
            let label = &project.keys[i].1;
            if !self.keys.contains_key(label) {
                let key = self.key(label);
                self.keys.insert(label.clone(), key);
            }
            if let Some(Some(key)) = self.keys.get(label) {
                out = Self::over(&self.place(&key.image, key.x, key.y, key.w, key.h), &out);
            }
        }
        out
    }
}

fn context() -> Retained<CIContext> {
    // Low priority keeps the rest of the Mac smooth while an export runs.
    let yes = NSNumber::new_bool(true);
    // SAFETY: an immutable framework constant used as a dictionary key.
    let options = unsafe {
        NSDictionary::<CIContextOption, AnyObject>::from_slices(&[kCIContextPriorityRequestLow], &[yes.as_ref()])
    };
    // SAFETY: creating a GPU context with valid options.
    unsafe { CIContext::contextWithOptions(Some(&options)) }
}

/// Where the file is written until it's complete: hidden, next to the final file, with the same extension.
fn partial(output: &Path) -> PathBuf {
    let name = output.file_stem().and_then(|s| s.to_str()).unwrap_or("export");
    let ext = output.extension().and_then(|s| s.to_str()).unwrap_or("mp4");
    output.with_file_name(format!(".{name}.partial.{ext}"))
}

/// Removes the partial file and the scratch files AVFoundation keeps beside it (`<name>.sb-…`).
fn remove_partial(temp: &Path) {
    let (Some(dir), Some(name)) = (temp.parent(), temp.file_name().and_then(|n| n.to_str())) else { return };
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if entry.file_name().to_str().is_some_and(|n| n.starts_with(name)) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

pub fn export(
    project: &RenderProject,
    options: &ExportOptions,
    progress: &mut dyn FnMut(f64) -> bool,
) -> Result<(), RenderError> {
    let screen = project.screen.as_ref().ok_or_else(|| RenderError::Project("there's no recording to export".into()))?;
    if options.format == Format::Gif {
        return Err(RenderError::Unsupported);
    }
    let (width, height) = (options.width as usize, options.height as usize);
    let layout = Layout::new(project, options.width, options.height);
    let mut compositor = Compositor::new(project, layout)?;
    let mut decoder = Decoder::open(screen)?;
    let context = context();
    let fps = options.fps.clamp(1, 120);
    let total = (project.output_duration() * f64::from(fps) - 1e-6).ceil().max(1.0) as u64;
    let last = project.clips.last().map_or(0.0, |c| (c.end - 1e-3).max(c.start));
    let temp = partial(&options.output);

    let started = std::time::Instant::now();
    let mut writer = VideoWriter::new(&temp, width, height, fps, options.format, options.bitrate)?;
    let result = (|| {
        for i in 0..total {
            if !progress(i as f64 / total as f64) {
                return Err(RenderError::Cancelled);
            }
            let t = project.source_time(i as f64 / f64::from(fps)).unwrap_or(last);
            autoreleasepool(|_| -> Result<(), RenderError> {
                let picture = decoder.frame_at(t)?.ok_or_else(|| system("the recording has no frames"))?;
                let frame = frame_at(project, &layout, t);
                let image = compositor.compose(&frame, &picture);
                writer.write(&context, &image, &compositor.space)
            })?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            if let Err(error) = writer.finish() {
                remove_partial(&temp);
                return Err(error);
            }
            std::fs::rename(&temp, &options.output)?;
            remove_partial(&temp);
            tracing::info!(frames = total, seconds = started.elapsed().as_secs_f64(), "exported {width}×{height}");
            progress(1.0);
            Ok(())
        }
        Err(error) => {
            writer.cancel();
            remove_partial(&temp);
            Err(error)
        }
    }
}
