//! The voices a project's captions transcribe, read as 16 kHz mono for Whisper: the recording's own voice (the
//! microphone, or the computer's sound when there's no microphone) and every voice-over.

use std::f64::consts::PI;
use std::path::Path;

use shotmaxxing_core::bundle::ProjectBundle;
use shotmaxxing_core::project::TrackKind;

use crate::RenderError;
use crate::voice::{Meter, MonoReader, VoiceSystem, quiet_spans};

/// Whisper's own rate.
pub const SPEECH_RATE: u32 = 16_000;

/// A voice to caption.
#[derive(Clone, Debug, PartialEq)]
pub struct Speech {
    /// The voice-over it is, by its file (as its track names it). `None` is the recording's own voice, on the
    /// recording's timeline; a voice-over is on the finished video's.
    pub voice_over: Option<String>,
    /// Where it starts on its timeline, in seconds.
    pub offset: f64,
    /// 16 kHz mono.
    pub samples: Vec<f32>,
    /// Stretches of a second or more where nothing is said, in seconds from its start.
    pub quiet: Vec<(f64, f64)>,
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Converts a stream of samples from one rate to another with a windowed sinc, which also filters out what the lower
/// rate can't carry. Each output sample is centred on its own moment, so nothing is delayed.
pub(crate) struct Resampler {
    /// Output sample `n` sits at input position `n * step_num / step_den`.
    step_num: u64,
    step_den: u64,
    /// Taps on each side of the centre, in input samples.
    half: i64,
    /// The filter for each fractional position (`step_den` of them), `2 * half` taps each.
    phases: Vec<Vec<f32>>,
    /// Input not yet used up, starting at input index `base`.
    input: Vec<f32>,
    base: u64,
    /// Input samples seen.
    seen: u64,
    /// The next output sample's index.
    next: u64,
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Self {
        let (from, to) = (u64::from(from.max(1)), u64::from(to.max(1)));
        let g = gcd(from, to);
        let (step_num, mut step_den) = (from / g, to / g);
        // Unusual rate pairs would need thousands of phases: close enough with 1024.
        let exact = step_den <= 1024;
        if !exact {
            step_den = 1024;
        }
        // Pass up to 90% of the lower rate's highest frequency; cycles per input sample.
        let cutoff = 0.45 * from.min(to) as f64 / from as f64;
        // Eight zero crossings of the filter on each side.
        let half = (8.0 / (2.0 * cutoff)).ceil() as i64;
        let phases = (0..step_den)
            .map(|p| {
                let frac = p as f64 / step_den as f64;
                let taps: Vec<f64> = (-half + 1..=half)
                    .map(|k| {
                        let x = k as f64 - frac;
                        let w = x / half as f64;
                        if w.abs() >= 1.0 {
                            return 0.0;
                        }
                        let a = 2.0 * PI * cutoff * x;
                        let sinc = if a.abs() < 1e-9 { 1.0 } else { a.sin() / a };
                        sinc * (0.5 + 0.5 * (PI * w).cos())
                    })
                    .collect();
                // Each phase passes a steady level unchanged.
                let sum: f64 = taps.iter().sum();
                taps.iter().map(|t| (t / sum) as f32).collect()
            })
            .collect();
        let step_num = if exact { step_num } else { (from as f64 / to as f64 * 1024.0).round() as u64 };
        Self { step_num, step_den, half, phases, input: Vec::new(), base: 0, seen: 0, next: 0 }
    }

    /// Adds `chunk` and appends every output sample it completes to `out`.
    pub fn push(&mut self, chunk: &[f32], out: &mut Vec<f32>) {
        self.input.extend_from_slice(chunk);
        self.seen += chunk.len() as u64;
        self.drain(out, self.seen);
    }

    /// Ends the stream: the last output samples are made as if silence followed.
    pub fn finish(mut self, out: &mut Vec<f32>) {
        let end = self.seen;
        self.input.extend(std::iter::repeat_n(0.0, self.half as usize + 1));
        self.seen += self.half as u64 + 1;
        // Only output samples whose moment falls inside the real input.
        while self.next * self.step_num / self.step_den < end && self.one(out) {}
    }

    fn drain(&mut self, out: &mut Vec<f32>, available: u64) {
        loop {
            let centre = (self.next * self.step_num / self.step_den) as i64;
            if centre + self.half >= available as i64 || !self.one(out) {
                break;
            }
        }
        // Keep only what the next output sample still needs.
        let centre = (self.next * self.step_num / self.step_den) as i64;
        let keep_from = (centre - self.half + 1).max(0) as u64;
        if keep_from > self.base {
            let drop = ((keep_from - self.base) as usize).min(self.input.len());
            self.input.drain(..drop);
            self.base += drop as u64;
        }
    }

    /// Makes the next output sample, if its input is all here.
    fn one(&mut self, out: &mut Vec<f32>) -> bool {
        let position = self.next * self.step_num;
        let centre = (position / self.step_den) as i64;
        let phase = &self.phases[(position % self.step_den) as usize];
        let end = self.base as i64 + self.input.len() as i64;
        if centre + self.half >= end {
            return false;
        }
        let mut sum = 0.0f32;
        for (j, k) in (centre - self.half + 1..=centre + self.half).enumerate() {
            // Before the first sample is silence.
            if k >= self.base as i64 {
                sum += self.input[(k - self.base as i64) as usize] * phase[j];
            }
        }
        out.push(sum);
        self.next += 1;
        true
    }
}

/// The voices to caption, with `system`'s sound files. `cancel` is checked as they're read.
#[cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]
pub(crate) fn speech_with(
    system: &impl VoiceSystem,
    root: &Path,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Speech>, RenderError> {
    let (bundle, project) = ProjectBundle::open(root).map_err(|e| RenderError::Project(e.to_string()))?;
    let root = bundle.root();
    let own = project
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Microphone)
        .or_else(|| project.tracks.iter().find(|t| t.kind == TrackKind::SystemAudio));
    let voice_overs = project.tracks.iter().filter(|t| t.kind == TrackKind::VoiceOver);
    let mut out = Vec::new();
    for track in own.into_iter().chain(voice_overs) {
        let mut reader = system.open(&root.join(&track.path))?;
        let rate = reader.rate();
        let mut meter = Meter::new(rate);
        let mut resampler = Resampler::new(rate, SPEECH_RATE);
        let mut samples = Vec::with_capacity(
            (reader.length() as f64 * f64::from(SPEECH_RATE) / f64::from(rate.max(1))) as usize,
        );
        while let Some(chunk) = reader.next()? {
            meter.push(chunk);
            resampler.push(chunk, &mut samples);
            if cancel() {
                return Err(RenderError::Cancelled);
            }
        }
        resampler.finish(&mut samples);
        out.push(Speech {
            voice_over: (track.kind == TrackKind::VoiceOver).then(|| track.path.clone()),
            offset: track.offset_us as f64 / 1e6,
            samples,
            quiet: quiet_spans(&meter.finish()),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, hz: f64, seconds: f64) -> Vec<f32> {
        (0..(f64::from(rate) * seconds) as usize)
            .map(|i| (2.0 * PI * hz * i as f64 / f64::from(rate)).sin() as f32)
            .collect()
    }

    fn resample(input: &[f32], from: u32, to: u32, chunk: usize) -> Vec<f32> {
        let mut resampler = Resampler::new(from, to);
        let mut out = Vec::new();
        for piece in input.chunks(chunk) {
            resampler.push(piece, &mut out);
        }
        resampler.finish(&mut out);
        out
    }

    fn rms(x: &[f32]) -> f64 {
        (x.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / x.len().max(1) as f64).sqrt()
    }

    #[test]
    fn a_tone_comes_through_at_the_new_rate_in_time() {
        for (from, chunk) in [(48_000, 4096), (44_100, 1000), (24_000, 333), (8_000, 512)] {
            let input = tone(from, 1000.0, 1.0);
            let out = resample(&input, from, SPEECH_RATE, chunk);
            assert!((out.len() as i64 - 16_000).abs() <= 1, "{from}: {} samples", out.len());
            // Away from the edges, every sample matches the same tone made at 16 kHz: nothing delayed or lost.
            let expected = tone(SPEECH_RATE, 1000.0, 1.0);
            let error: Vec<f32> =
                out[200..15_800].iter().zip(&expected[200..15_800]).map(|(a, b)| a - b).collect();
            assert!(rms(&error) < 0.01, "{from}: error {}", rms(&error));
        }
    }

    #[test]
    fn what_16_khz_cannot_carry_is_filtered_out() {
        // 10 kHz is above 16 kHz's 8 kHz limit: kept, it would fold back as a 6 kHz whistle.
        let out = resample(&tone(48_000, 10_000.0, 0.5), 48_000, SPEECH_RATE, 4096);
        assert!(rms(&out[200..out.len() - 200]) < 0.01, "{}", rms(&out));
        // A steady level passes unchanged.
        let out = resample(&vec![0.5; 48_000], 48_000, SPEECH_RATE, 4096);
        assert!(out[100..15_900].iter().all(|v| (v - 0.5).abs() < 1e-3));
    }

    #[test]
    fn nothing_in_nothing_out() {
        assert!(resample(&[], 48_000, SPEECH_RATE, 1024).is_empty());
    }
}
