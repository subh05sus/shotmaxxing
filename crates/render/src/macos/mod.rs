//! The macOS export. AVFoundation decodes the recording in hardware, Core Image draws each frame on the GPU, and
//! AVFoundation encodes it in hardware. Only the still parts (wallpaper, shadow, cursor, labels) are drawn on the
//! CPU, once per export.
//!
//! Every layer is placed straight into the output frame with the camera applied, rather than drawing the canvas
//! first and zooming it afterwards. A zoom therefore shows the recording's real pixels, not an enlarged copy, and the
//! cursor and arrows, drawn at the largest zoom, stay sharp.

mod decode;
mod sound;

pub use sound::audio_seconds;
mod text;
pub(crate) use text::draw as draw_text;
mod video;
pub mod voice;

use std::collections::HashMap;
use std::path::Path;
use std::ptr::NonNull;

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2_core_foundation::{CFRetained, CGAffineTransform, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGColorSpace, kCGColorSpaceSRGB};
use objc2_core_image::{
    CIColor, CIContext, CIContextOption, CIFilter, CIImage, CIVector, kCIContextPriorityRequestLow,
    kCIContextWorkingColorSpace, kCIFormatRGBA8,
};
use objc2_core_video::CVPixelBuffer;
use objc2_foundation::{NSData, NSDictionary, NSNumber, NSString, ns_string};
use shotmaxxing_core::geometry::Point;
use tiny_skia::Pixmap;

use self::decode::{Decoder, system};
use self::sound::Sound;
use self::video::VideoWriter;
use crate::model::{Bubble, RenderProject};
use crate::scene::{Frame, Layout, frame_at};
use crate::sprites::{self, LINE_HEIGHT, Label, Palette, Sprite};
use crate::{ExportOptions, Format, RenderError, animated, soft, wallpaper};

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
    let (w, h) =
        (f64::from(sprite.pixmap.width()) / sprite.scale, f64::from(sprite.pixmap.height()) / sprite.scale);
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
        let css =
            wallpaper::find(&project.wallpaper).map(|w| w.css).ok_or_else(|| system("no wallpapers"))?;
        let pixels = wallpaper::rasterize(&css, w, h).map_err(RenderError::Project)?;
        let background = sprites::background(&layout, pixels, &palette)
            .ok_or_else(|| system("couldn't draw the background"))?;
        let background = to_ci(&background, &space);
        // Drawn at the largest zoom, so they're never enlarged.
        let res = project.max_zoom().clamp(1.0, 4.0);
        let cursor =
            sprites::cursor(project.cursor_look, layout.cursor_px, res, &palette).map(|s| placed(s, &space));
        let ring = sprites::ring(layout.cursor_px, res, &palette).map(|s| placed(s, &space));
        let spotlight = sprites::spotlight(layout.cursor_px, res, &palette).map(|s| placed(s, &space));
        let arrows = project
            .arrows
            .iter()
            .map(|a| {
                let (from, to) = (layout.point_to_frame(a.from), layout.point_to_frame(a.to));
                sprites::arrow(&layout, (from.x, from.y), (to.x, to.y), res, &palette)
                    .map(|s| placed(s, &space))
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
        let glyphs = text::draw(text, self.layout.text_px, LINE_HEIGHT, self.palette.accent_ink)?;
        sprites::title(&self.layout, &glyphs, at, &self.palette).map(|label| self.label(label))
    }

    /// A keystroke: centred near the bottom on a dark scrim.
    fn key(&self, label: &str) -> Option<Placed> {
        let glyphs = text::draw(label, self.layout.text_px, LINE_HEIGHT, self.palette.on_stage)?;
        sprites::key(&self.layout, &glyphs, &self.palette).map(|label| self.label(label))
    }

    fn label(&self, label: Label) -> Placed {
        let (w, h) = (f64::from(label.pixmap.width()), f64::from(label.pixmap.height()));
        Placed { image: to_ci(&label.pixmap, &self.space), x: label.x, y: label.y, w, h }
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
        unsafe {
            image.imageByClampingToExtent().imageByApplyingTransform_highQualityDownsample(t, downsample)
        }
    }

    fn transform(&self, image: &CIImage, x: f64, y: f64, w: f64, h: f64) -> (CGAffineTransform, bool) {
        // SAFETY: reading an image's extent is a pure operation.
        let e = unsafe { image.extent() };
        let (sx, sy) = (w / e.size.width, h / e.size.height);
        let (dx, dy) = (x, self.layout.height - y - h);
        let t = CGAffineTransform {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            tx: dx - e.origin.x * sx,
            ty: dy - e.origin.y * sy,
        };
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

    /// Adds two premultiplied images, for averaging copies.
    fn add(top: &CIImage, bottom: &CIImage) -> Retained<CIImage> {
        // SAFETY: a built-in filter with valid inputs.
        unsafe {
            top.imageByApplyingFilter_withInputParameters(
                ns_string!("CIAdditionCompositing"),
                &params(&[(ns_string!("inputBackgroundImage"), bottom.as_ref())]),
            )
        }
    }

    /// The cursor smeared along its path during the shutter: copies from where it was (`back`, an offset from its
    /// place now) to where it is, averaged so the smear never gets darker than the cursor, with more weight at the
    /// front so it stays easy to follow.
    fn smear(
        &self,
        image: &CIImage,
        (x, y, w, h): (f64, f64, f64, f64),
        back: (f64, f64),
    ) -> Retained<CIImage> {
        let copies = (back.0.hypot(back.1) / (w.max(1.0) * 0.12)).ceil().clamp(2.0, 10.0) as usize;
        let weight = |i: usize| 1.0 + 2.0 * i as f64 / (copies - 1) as f64;
        let total: f64 = (0..copies).map(weight).sum();
        let mut layer: Option<Retained<CIImage>> = None;
        for i in 0..copies {
            let behind = 1.0 - i as f64 / (copies - 1) as f64;
            let copy = Self::opacity(
                self.place(image, x + back.0 * behind, y + back.1 * behind, w, h),
                weight(i) / total,
            );
            layer = Some(match layer {
                Some(layer) => Self::add(&copy, &layer),
                None => copy,
            });
        }
        layer.unwrap_or_else(|| self.place(image, x, y, w, h))
    }

    /// `image`'s shape in black at `alpha`, for shadows.
    fn shade(image: &CIImage, alpha: f64) -> Retained<CIImage> {
        // SAFETY: a built-in filter with valid inputs.
        unsafe {
            let zero = CIVector::vectorWithX_Y_Z_W(0.0, 0.0, 0.0, 0.0);
            let a = CIVector::vectorWithX_Y_Z_W(0.0, 0.0, 0.0, alpha);
            image.imageByApplyingFilter_withInputParameters(
                ns_string!("CIColorMatrix"),
                &params(&[
                    (ns_string!("inputRVector"), zero.as_ref()),
                    (ns_string!("inputGVector"), zero.as_ref()),
                    (ns_string!("inputBVector"), zero.as_ref()),
                    (ns_string!("inputAVector"), a.as_ref()),
                ]),
            )
        }
    }

    /// The camera bubble in its corner, over everything else, sized, ringed and shaded as in the editor: a ring of
    /// 0.4% of the frame height, a shadow 0.8% down with a 1.2% blur, 4% from the edges, and 70% of its size at
    /// full zoom when it shrinks.
    fn bubble(
        &self,
        bubble: &Bubble,
        camera: &CVPixelBuffer,
        zoom: f64,
        out: Retained<CIImage>,
    ) -> Retained<CIImage> {
        let l = self.layout;
        let shrink = if bubble.shrink { 1.0 - 0.3 * ((zoom - 1.0) / 0.5).clamp(0.0, 1.0) } else { 1.0 };
        let d = l.height * bubble.size * shrink;
        let margin = l.height * 0.04;
        let x = if bubble.left { margin } else { l.width - d - margin };
        let y = if bubble.top { margin } else { l.height - d - margin };
        let border = l.height * 0.004;
        let radius = d * bubble.radius;
        let mut out = out;
        if bubble.shadow
            && let Some(shape) = self.rounded(self.rect(x, y + l.height * 0.008, d, d), radius)
        {
            // SAFETY: a pure Core Image operation.
            let shadow =
                unsafe { Self::shade(&shape, 0.35).imageByApplyingGaussianBlurWithSigma(l.height * 0.012) };
            out = Self::over(&shadow, &out);
        }
        if let Some(ring) = self.rounded(self.rect(x, y, d, d), radius) {
            out = Self::over(&ring, &out);
        }
        // SAFETY: the buffer comes from the decoder and stays alive for this frame; the rest is pure Core Image.
        let picture = unsafe {
            let picture = CIImage::imageWithCVPixelBuffer(camera);
            // The middle square of the camera's picture, like `object-fit: cover`.
            let e = picture.extent();
            let side = e.size.width.min(e.size.height);
            let (sx, sy) =
                (e.origin.x + (e.size.width - side) / 2.0, e.origin.y + (e.size.height - side) / 2.0);
            let square =
                picture.imageByCroppingToRect(CGRect::new(CGPoint::new(sx, sy), CGSize::new(side, side)));
            if bubble.mirror {
                let flip =
                    CGAffineTransform { a: -1.0, b: 0.0, c: 0.0, d: 1.0, tx: 2.0 * sx + side, ty: 0.0 };
                square.imageByApplyingTransform(flip)
            } else {
                square
            }
        };
        let inner = d - 2.0 * border;
        let placed = self.place_edgeless(&picture, x + border, y + border, inner, inner);
        match self.rounded(self.rect(x + border, y + border, inner, inner), (radius - border).max(0.0)) {
            Some(mask) => Self::masked(&placed, &out, &mask),
            None => out,
        }
    }

    /// One finished frame: the recording's picture `screen` with everything the project adds at this moment.
    fn compose(
        &mut self,
        frame: &Frame,
        screen: &CVPixelBuffer,
        camera: Option<&CVPixelBuffer>,
    ) -> Retained<CIImage> {
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
                out = match c.from {
                    Some(from) => Self::over(
                        &self.smear(
                            &cursor.image,
                            (x, y, w, h),
                            (v.tx + s * from.x - hx, v.ty + s * from.y - hy),
                        ),
                        &out,
                    ),
                    None => Self::over(&self.place(&cursor.image, x, y, w, h), &out),
                };
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
        if let (Some(bubble), Some(camera)) = (&project.bubble, camera) {
            out = self.bubble(bubble, camera, s, out);
        }
        out
    }
}

fn context() -> Result<Retained<CIContext>, RenderError> {
    // Low priority keeps the rest of the Mac smooth while an export runs. Blending in sRGB, as browsers do, keeps
    // glows, soft shadows and blurs as they look in the editor; by default Core Image blends in linear light, which
    // makes them larger and brighter.
    let yes = NSNumber::new_bool(true);
    let space = CGColorSpace::with_name(Some(
        // SAFETY: an immutable framework constant.
        unsafe { kCGColorSpaceSRGB },
    ))
    .ok_or_else(|| system("the sRGB colour space is missing"))?;
    let raw: *const CGColorSpace = &*space;
    // SAFETY: Core Foundation objects are Objective-C objects (toll-free bridged), so a colour space can be a
    // dictionary value; the dictionary retains it. The keys are immutable framework constants.
    let options = unsafe {
        NSDictionary::<CIContextOption, AnyObject>::from_slices(
            &[kCIContextPriorityRequestLow, kCIContextWorkingColorSpace],
            &[yes.as_ref(), &*raw.cast::<AnyObject>()],
        )
    };
    // SAFETY: creating a GPU context with valid options.
    Ok(unsafe { CIContext::contextWithOptions(Some(&options)) })
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

/// Draws `image` into RGBA bytes, top row first.
fn render_rgba(
    context: &CIContext,
    image: &CIImage,
    width: usize,
    height: usize,
    space: &CGColorSpace,
    out: &mut Vec<u8>,
) {
    out.resize(width * height * 4, 0);
    let bounds = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(width as f64, height as f64));
    // SAFETY: `out` holds exactly `height` rows of `width` RGBA pixels and outlives the call.
    unsafe {
        context.render_toBitmap_rowBytes_bounds_format_colorSpace(
            image,
            NonNull::new_unchecked(out.as_mut_ptr().cast()),
            (width * 4) as isize,
            bounds,
            kCIFormatRGBA8,
            Some(space),
        );
    }
}

/// Everything one export needs to draw its frames, in order.
struct Job<'a> {
    project: &'a RenderProject,
    layout: Layout,
    compositor: Compositor<'a>,
    decoder: Decoder,
    /// The camera recording, when the bubble shows.
    camera: Option<Decoder>,
    context: Retained<CIContext>,
    fps: u32,
    /// Frames in the exported video.
    total: u64,
    /// Where the recording ends, for a last frame that rounding pushes past the final clip.
    last: f64,
}

impl<'a> Job<'a> {
    fn new(project: &'a RenderProject, options: &ExportOptions) -> Result<Self, RenderError> {
        Self::starting(project, options, 0.0)
    }

    /// A job whose first frame is at `t` seconds into the recording: its decoders start just before it.
    fn starting(project: &'a RenderProject, options: &ExportOptions, t: f64) -> Result<Self, RenderError> {
        let screen = project
            .screen
            .as_ref()
            .ok_or_else(|| RenderError::Project("there's no recording to export".into()))?;
        let layout = Layout::new(project, options.width, options.height);
        let fps =
            if options.format == Format::Gif { options.fps.clamp(1, 50) } else { options.fps.clamp(1, 120) };
        Ok(Self {
            project,
            layout,
            compositor: Compositor::new(project, layout)?,
            decoder: Decoder::open_near(screen, t)?,
            camera: project.bubble.as_ref().map(|b| Decoder::open_near(&b.path, t - b.offset)).transpose()?,
            context: context()?,
            fps,
            total: (project.output_duration() * f64::from(fps) - 1e-6).ceil().max(1.0) as u64,
            last: project.clips.last().map_or(0.0, |c| (c.end - 1e-3).max(c.start)),
        })
    }

    /// The moment of the recording that frame `i` shows.
    fn time(&self, i: u64) -> f64 {
        self.project.source_time(i as f64 / f64::from(self.fps)).unwrap_or(self.last)
    }

    /// Draws frame `i`. Frames must be asked for in order.
    fn draw(&mut self, i: u64) -> Result<Retained<CIImage>, RenderError> {
        let t = self.time(i);
        self.draw_at(t)
    }

    /// Draws the moment `t` seconds into the recording. Moments must be asked for in order.
    fn draw_at(&mut self, t: f64) -> Result<Retained<CIImage>, RenderError> {
        let picture = self.decoder.frame_at(t)?.ok_or_else(|| system("the recording has no frames"))?;
        let camera = match (&mut self.camera, &self.project.bubble) {
            (Some(decoder), Some(bubble)) => decoder.frame_at(t - bubble.offset)?,
            _ => None,
        };
        let frame = frame_at(self.project, &self.layout, t);
        Ok(self.compositor.compose(&frame, &picture, camera.as_deref()))
    }
}

pub fn export(
    project: &RenderProject,
    options: &ExportOptions,
    progress: &mut dyn FnMut(f64) -> bool,
) -> Result<(), RenderError> {
    let temp = crate::partial(&options.output);
    let started = std::time::Instant::now();
    let result = match options.format {
        Format::Gif => export_gif(project, options, &temp, progress),
        Format::H264 | Format::Hevc => export_video(project, options, &temp, progress),
        Format::Png => export_still(project, options, &temp),
    };
    match result {
        Ok(frames) => {
            std::fs::rename(&temp, &options.output)?;
            remove_partial(&temp);
            let seconds = started.elapsed().as_secs_f64();
            tracing::info!(frames, seconds, "exported {}×{}", options.width, options.height);
            progress(1.0);
            Ok(())
        }
        Err(error) => {
            remove_partial(&temp);
            Err(error)
        }
    }
}

fn export_video(
    project: &RenderProject,
    options: &ExportOptions,
    temp: &Path,
    progress: &mut dyn FnMut(f64) -> bool,
) -> Result<u64, RenderError> {
    let mut job = Job::new(project, options)?;
    let (width, height) = (options.width as usize, options.height as usize);
    let mut sound = Sound::open(project)?;
    let mut writer =
        VideoWriter::new(temp, width, height, job.fps, options.format, options.bitrate, sound.is_some())?;
    let drawn = (|| {
        for i in 0..job.total {
            if !progress(i as f64 / job.total as f64) {
                return Err(RenderError::Cancelled);
            }
            autoreleasepool(|_| -> Result<(), RenderError> {
                let image = job.draw(i)?;
                writer.write(&job.context, &image, &job.compositor.space, sound.as_mut())
            })?;
        }
        Ok(())
    })();
    match drawn {
        Ok(()) => writer.finish(sound.as_mut()).map(|()| job.total),
        Err(error) => {
            writer.cancel();
            Err(error)
        }
    }
}

/// One moment, styled like the video, as a PNG.
fn export_still(project: &RenderProject, options: &ExportOptions, temp: &Path) -> Result<u64, RenderError> {
    let t = project.kept_time(options.at.unwrap_or(0.0).max(0.0));
    let mut job = Job::starting(project, options, t)?;
    let mut rgba = Vec::new();
    autoreleasepool(|_| -> Result<(), RenderError> {
        let image = job.draw_at(t)?;
        let (width, height) = (options.width as usize, options.height as usize);
        render_rgba(&job.context, &image, width, height, &job.compositor.space, &mut rgba);
        Ok(())
    })?;
    let size = tiny_skia::IntSize::from_wh(options.width, options.height)
        .ok_or_else(|| RenderError::Project("that size can't be drawn".into()))?;
    // Core Image hands back premultiplied pixels, which is what a pixmap holds; the PNG gets straight alpha.
    let pixmap =
        tiny_skia::Pixmap::from_vec(rgba, size).ok_or_else(|| system("the frame couldn't be kept"))?;
    let png = pixmap.encode_png().map_err(|e| system(format!("couldn't make the PNG: {e}")))?;
    std::fs::write(temp, png)?;
    Ok(1)
}

/// Draws one moment with Core Image and with the portable compositor, from the same decoded pictures, as RGBA
/// frames. It lets tests hold the portable compositor to the Mac's.
pub fn draw_both(
    project: &RenderProject,
    options: &ExportOptions,
) -> Result<(Vec<u8>, Vec<u8>), RenderError> {
    let t = project.kept_time(options.at.unwrap_or(0.0).max(0.0));
    let mut job = Job::starting(project, options, t)?;
    let (width, height) = (options.width as usize, options.height as usize);
    autoreleasepool(|_| {
        let screen = job.decoder.frame_at(t)?.ok_or_else(|| system("the recording has no frames"))?;
        let camera = match (&mut job.camera, &project.bubble) {
            (Some(decoder), Some(bubble)) => decoder.frame_at(t - bubble.offset)?,
            _ => None,
        };
        let frame = frame_at(project, &job.layout, t);
        let image = job.compositor.compose(&frame, &screen, camera.as_deref());
        let mut core = Vec::new();
        render_rgba(&job.context, &image, width, height, &job.compositor.space, &mut core);

        let pixmap = |buffer: &CVPixelBuffer| -> Result<Pixmap, RenderError> {
            // SAFETY: the buffer comes from the decoder and stays alive for this frame.
            let image = unsafe { CIImage::imageWithCVPixelBuffer(buffer) };
            // SAFETY: reading an image's extent is a pure operation.
            let size = unsafe { image.extent() }.size;
            let (w, h) = (size.width as usize, size.height as usize);
            let mut rgba = Vec::new();
            render_rgba(&job.context, &image, w, h, &job.compositor.space, &mut rgba);
            let size =
                tiny_skia::IntSize::from_wh(w as u32, h as u32).ok_or_else(|| system("an empty picture"))?;
            Pixmap::from_vec(rgba, size).ok_or_else(|| system("the picture couldn't be kept"))
        };
        let screen = pixmap(&screen)?;
        let camera = camera.as_deref().map(pixmap).transpose()?;
        let mut portable = soft::Compositor::new(project, job.layout, text::draw)?;
        let drawn = portable.compose(&frame, screen.as_ref(), camera.as_ref().map(Pixmap::as_ref));
        Ok((core, drawn.data().to_vec()))
    })
}

fn export_gif(
    project: &RenderProject,
    options: &ExportOptions,
    temp: &Path,
    progress: &mut dyn FnMut(f64) -> bool,
) -> Result<u64, RenderError> {
    let (width, height) = (options.width as usize, options.height as usize);
    if width > usize::from(u16::MAX) || height > usize::from(u16::MAX) {
        return Err(RenderError::Project("that's too large for a GIF".into()));
    }
    let mut rgba = Vec::new();

    // A first, quick pass over a few frames finds the colours the whole clip needs.
    let mut sampler = Job::new(project, options)?;
    let count = animated::PALETTE_SAMPLES.min(sampler.total);
    let mut samples = Vec::new();
    for k in 0..count {
        if !progress(0.08 * k as f64 / count as f64) {
            return Err(RenderError::Cancelled);
        }
        autoreleasepool(|_| -> Result<(), RenderError> {
            let image = sampler.draw(k * sampler.total / count)?;
            render_rgba(&sampler.context, &image, width, height, &sampler.compositor.space, &mut rgba);
            // Every other row is plenty to learn colours from.
            for row in rgba.chunks_exact(width * 4).step_by(2) {
                samples.extend_from_slice(row);
            }
            Ok(())
        })?;
    }
    drop(sampler);

    let mut job = Job::new(project, options)?;
    let file = std::io::BufWriter::new(std::fs::File::create(temp)?);
    let mut writer = animated::GifWriter::new(file, width as u16, height as u16, job.fps, &samples)?;
    drop(samples);
    for i in 0..job.total {
        if !progress(0.08 + 0.92 * i as f64 / job.total as f64) {
            return Err(RenderError::Cancelled);
        }
        autoreleasepool(|_| -> Result<(), RenderError> {
            let image = job.draw(i)?;
            render_rgba(&job.context, &image, width, height, &job.compositor.space, &mut rgba);
            writer.write(&rgba)
        })?;
    }
    let mut file = writer.finish()?;
    std::io::Write::flush(&mut file)?;
    Ok(job.total)
}
