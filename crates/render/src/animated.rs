//! GIF encoding for exports.
//!
//! Screen recordings make poor GIFs when each frame gets its own palette: flat areas shimmer and nothing compresses.
//! Here the whole clip shares one palette, learned from frames across it; colours are spread with an ordered (Bayer)
//! pattern that stays put from frame to frame; and each frame stores only the rectangle that changed, with
//! unchanged pixels left transparent. A mostly still screen costs almost nothing per frame.

use std::borrow::Cow;
use std::io::Write;

use color_quant::NeuQuant;
use gif::{DisposalMethod, Encoder, Frame, Repeat};

use crate::RenderError;

/// Frames sampled across the clip to learn a GIF's palette from.
pub const PALETTE_SAMPLES: u64 = 16;

/// The palette index that means "unchanged".
const CLEAR: u8 = 255;
/// Colours in the palette, leaving one index for "unchanged".
const COLOURS: usize = 255;
/// Bits per channel of the nearest-colour cache.
const CACHE_BITS: u32 = 6;
/// How far the dither pattern nudges a channel, in 8-bit steps. Enough to break up bands in gradients, small
/// enough that flat interface colours stay flat.
const DITHER: f32 = 10.0;

const BAYER: [[u8; 8]; 8] = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

fn gif_error(error: gif::EncodingError) -> RenderError {
    match error {
        gif::EncodingError::Io(io) => RenderError::Io(io),
        other => RenderError::System(format!("couldn't write the GIF: {other}")),
    }
}

pub struct GifWriter<W: Write> {
    encoder: Encoder<W>,
    width: usize,
    height: usize,
    palette: Vec<[i32; 3]>,
    /// Nearest palette index for each 6-bit colour, filled in as colours turn up. `u16::MAX` is "not yet".
    cache: Vec<u16>,
    previous: Option<Vec<u8>>,
    fps: f64,
    /// What rounding the delays to whole hundredths of a second has gained or lost so far.
    drift: f64,
    /// Indices for the frame being written, kept to save an allocation per frame.
    indices: Vec<u8>,
}

impl<W: Write> GifWriter<W> {
    /// `samples` is RGBA pixels from frames across the clip, to learn the palette from.
    pub fn new(out: W, width: u16, height: u16, fps: u32, samples: &[u8]) -> Result<Self, RenderError> {
        if samples.len() < 4 * COLOURS {
            return Err(RenderError::System("not enough picture to make a GIF palette from".into()));
        }
        let quant = NeuQuant::new(10, COLOURS, samples);
        let rgb = quant.color_map_rgb();
        let palette: Vec<[i32; 3]> = rgb
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| [i32::from(c[0]), i32::from(c[1]), i32::from(c[2])])
            .collect();
        let mut global = rgb;
        global.resize(256 * 3, 0);
        let mut encoder = Encoder::new(out, width, height, &global).map_err(gif_error)?;
        encoder.set_repeat(Repeat::Infinite).map_err(gif_error)?;
        let (width, height) = (usize::from(width), usize::from(height));
        Ok(Self {
            encoder,
            width,
            height,
            palette,
            cache: vec![u16::MAX; 1 << (CACHE_BITS * 3)],
            previous: None,
            fps: f64::from(fps.clamp(1, 50)),
            drift: 0.0,
            indices: vec![0; width * height],
        })
    }

    fn nearest(&mut self, r: u8, g: u8, b: u8) -> u8 {
        let shift = 8 - CACHE_BITS;
        let key = (usize::from(r >> shift) << (CACHE_BITS * 2))
            | (usize::from(g >> shift) << CACHE_BITS)
            | usize::from(b >> shift);
        let cached = self.cache[key];
        if cached != u16::MAX {
            return cached as u8;
        }
        // The centre of the cache cell, matched with weights roughly following how sensitive eyes are to each channel.
        let half = 1 << (shift - 1);
        let (r, g, b) = (
            i32::from(r >> shift << shift) + half,
            i32::from(g >> shift << shift) + half,
            i32::from(b >> shift << shift) + half,
        );
        let mut best = (i32::MAX, 0usize);
        for (i, c) in self.palette.iter().enumerate() {
            let (dr, dg, db) = (r - c[0], g - c[1], b - c[2]);
            let d = 2 * dr * dr + 4 * dg * dg + 3 * db * db;
            if d < best.0 {
                best = (d, i);
            }
        }
        self.cache[key] = best.1 as u16;
        best.1 as u8
    }

    /// Adds one frame of RGBA pixels (alpha is ignored; exports are opaque).
    pub fn write(&mut self, rgba: &[u8]) -> Result<(), RenderError> {
        let (w, h) = (self.width, self.height);
        if rgba.len() < w * h * 4 {
            return Err(RenderError::System("a GIF frame was the wrong size".into()));
        }
        let mut indices = std::mem::take(&mut self.indices);
        for y in 0..h {
            let row = &BAYER[y % 8];
            for x in 0..w {
                let nudge = (f32::from(row[x % 8]) / 64.0 - 0.5) * DITHER;
                let p = &rgba[(y * w + x) * 4..(y * w + x) * 4 + 3];
                let channel = |v: u8| (f32::from(v) + nudge).round().clamp(0.0, 255.0) as u8;
                indices[y * w + x] = self.nearest(channel(p[0]), channel(p[1]), channel(p[2]));
            }
        }

        // Only the part that changed is stored; the rest shows through from earlier frames.
        let (mut left, mut top, mut right, mut bottom) = (0, 0, w, h);
        if let Some(previous) = &self.previous {
            let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
            for y in 0..h {
                for x in 0..w {
                    if indices[y * w + x] != previous[y * w + x] {
                        x0 = x0.min(x);
                        x1 = x1.max(x + 1);
                        y0 = y0.min(y);
                        y1 = y1.max(y + 1);
                    }
                }
            }
            (left, top, right, bottom) = if x1 == 0 { (0, 0, 1, 1) } else { (x0, y0, x1, y1) };
        }
        let mut buffer = Vec::with_capacity((right - left) * (bottom - top));
        for y in top..bottom {
            for x in left..right {
                let i = y * w + x;
                let unchanged = self.previous.as_ref().is_some_and(|p| p[i] == indices[i]);
                buffer.push(if unchanged { CLEAR } else { indices[i] });
            }
        }

        let exact = 100.0 / self.fps + self.drift;
        let delay = exact.round().max(2.0);
        self.drift = exact - delay;
        let frame = Frame {
            delay: delay as u16,
            dispose: DisposalMethod::Keep,
            transparent: Some(CLEAR),
            left: left as u16,
            top: top as u16,
            width: (right - left) as u16,
            height: (bottom - top) as u16,
            buffer: Cow::Owned(buffer),
            ..Frame::default()
        };
        self.encoder.write_frame(&frame).map_err(gif_error)?;
        self.indices = self.previous.replace(indices).unwrap_or_else(|| vec![0; w * h]);
        Ok(())
    }

    pub fn finish(self) -> Result<W, RenderError> {
        self.encoder.into_inner().map_err(RenderError::Io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: usize, h: usize, shift: u8) -> Vec<u8> {
        let mut px = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                px.extend_from_slice(&[(x * 255 / w) as u8, (y * 255 / h) as u8, shift, 255]);
            }
        }
        px
    }

    #[test]
    fn still_frames_cost_almost_nothing() {
        let (w, h) = (64usize, 48usize);
        let frame = gradient(w, h, 90);
        let mut writer = GifWriter::new(Vec::new(), w as u16, h as u16, 15, &frame).unwrap();
        writer.write(&frame).unwrap();
        let after_first = writer.encoder.get_ref().len();
        for _ in 0..10 {
            writer.write(&frame).unwrap();
        }
        let after_still = writer.encoder.get_ref().len();
        // Each repeated frame is a single clear pixel plus headers.
        assert!(after_still - after_first < 10 * 40, "{} bytes for still frames", after_still - after_first);
        let bytes = writer.finish().unwrap();
        assert_eq!(&bytes[..6], b"GIF89a");
        assert_eq!(*bytes.last().unwrap(), 0x3b);
    }

    #[test]
    fn delays_average_out_to_the_frame_rate() {
        let frame = gradient(32, 32, 10);
        let mut writer = GifWriter::new(Vec::new(), 32, 32, 15, &frame).unwrap();
        let mut total = 0.0;
        for _ in 0..30 {
            let exact = 100.0 / writer.fps + writer.drift;
            total += exact.round().max(2.0);
            writer.write(&frame).unwrap();
        }
        // Thirty frames at 15 fps is two seconds: 200 hundredths, give or take one.
        assert!((total - 200.0f64).abs() <= 1.0, "{total}");
    }
}
