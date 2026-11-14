//! Voice tools that run on this computer: finding long pauses, and cleaning up a recorded voice.
//!
//! Cleanup is the chain a sound engineer would reach for, kept gentle so a voice still sounds like itself:
//! - a high-pass filter takes out rumble, desk thumps and the fundamental of mains hum;
//! - a spectral noise filter learns what stays steady in the background (fans, hiss, hum harmonics) by tracking the
//!   quietest moments of each frequency, and turns that down without touching the voice above it;
//! - a slow leveller evens out loud and quiet passages;
//! - a soft limiter keeps peaks under full scale.
//!
//! Everything streams: memory stays flat however long the take is, and the output is exactly as long as the input,
//! so the voice stays in sync with the picture.

use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use shotmaxxing_core::telemetry::CursorSample;

/// Keeps peaks under full scale without the harshness of clipping: untouched below the knee, rounded off above.
pub fn soft_limit(x: f32) -> f32 {
    const KNEE: f32 = 0.9;
    let level = x.abs();
    if level <= KNEE { x } else { (KNEE + (1.0 - KNEE) * ((level - KNEE) / (1.0 - KNEE)).tanh()).copysign(x) }
}

// Pauses -------------------------------------------------------------------------------------------------------

/// Loudness is measured in windows this long, in seconds.
pub const WINDOW: f64 = 0.02;
/// Pauses at least this long get shortened.
pub const LONG_PAUSE: f64 = 1.0;
/// A shortened pause keeps this much at each end, so it still reads as a breath.
pub const KEEP: f64 = 0.2;

/// Measures loudness in 20 ms windows as samples stream in.
pub struct Meter {
    size: usize,
    power: f64,
    count: usize,
    levels: Vec<f32>,
}

impl Meter {
    pub fn new(rate: u32) -> Self {
        let size = ((f64::from(rate) * WINDOW).round() as usize).max(1);
        Self { size, power: 0.0, count: 0, levels: Vec::new() }
    }

    pub fn push(&mut self, samples: &[f32]) {
        for &x in samples {
            self.power += f64::from(x) * f64::from(x);
            self.count += 1;
            if self.count == self.size {
                self.levels.push(decibels(self.power / self.size as f64));
                self.power = 0.0;
                self.count = 0;
            }
        }
    }

    /// Each window's loudness in dB below full scale.
    pub fn finish(mut self) -> Vec<f32> {
        if self.count > 0 {
            self.levels.push(decibels(self.power / self.count as f64));
        }
        self.levels
    }
}

fn decibels(power: f64) -> f32 {
    (10.0 * power.max(1e-12).log10()) as f32
}

/// The quiet stretches of a voice recording, in seconds of that recording. The threshold adapts to the take: a
/// quarter of the way from the room's noise up to the voice. A take without a clear voice has no pauses to find.
pub fn quiet_spans(levels: &[f32]) -> Vec<(f64, f64)> {
    let len = levels.len();
    if len < 50 {
        return Vec::new();
    }
    let mut sorted = levels.to_vec();
    sorted.sort_by(f32::total_cmp);
    let floor = sorted[len / 10];
    let voice = sorted[len * 95 / 100];
    if voice - floor < 10.0 {
        return Vec::new();
    }
    let threshold = floor + ((voice - floor) * 0.25).max(6.0);
    // Speech, widened by 100 ms on each side so the tail of a word never counts as quiet.
    let hang = (0.1 / WINDOW).round() as usize;
    let mut speech = vec![false; len];
    for (i, _) in levels.iter().enumerate().filter(|(_, l)| **l >= threshold) {
        speech[i.saturating_sub(hang)..=(i + hang).min(len - 1)].fill(true);
    }
    let mut spans = Vec::new();
    let mut i = 0;
    while i < len {
        if speech[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < len && !speech[i] {
            i += 1;
        }
        spans.push((start as f64 * WINDOW, i as f64 * WINDOW));
    }
    spans
}

/// What happened on screen, to tell a pause that's just dead air from one where the person is doing something.
pub struct Activity<'a> {
    /// Times of clicks, keys, scrolls and app switches, in seconds.
    pub inputs: &'a [f64],
    pub cursor: &'a [CursorSample],
    /// Times of the frames the screen recording stored; it only stores one when something changed.
    pub frames: &'a [f64],
}

impl Activity<'_> {
    /// Whether anything happens between `start` and `end`: an input, the pointer travelling more than 60 points,
    /// or the screen animating (more than six changes a second, which a blinking text cursor stays under).
    pub fn busy(&self, start: f64, end: f64) -> bool {
        if self.inputs.iter().any(|&t| t >= start - 0.15 && t <= end + 0.15) {
            return true;
        }
        let sec = |s: &CursorSample| s.t_us as f64 / 1e6;
        let travel: f64 = self
            .cursor
            .windows(2)
            .filter(|w| sec(&w[1]) >= start && sec(&w[0]) <= end)
            .map(|w| w[0].point().distance(w[1].point()))
            .sum();
        if travel > 60.0 {
            return true;
        }
        let first = self.frames.partition_point(|&t| t < start);
        let last = self.frames.partition_point(|&t| t <= end);
        (last - first) as f64 > 6.0 * (end - start)
    }
}

/// The parts of long pauses to cut, in seconds of the screen recording. `offset` places the voice on the screen's
/// timeline. A pause where something happens on screen stays whole.
pub fn pause_cuts(
    spans: &[(f64, f64)],
    offset: f64,
    duration: f64,
    busy: &dyn Fn(f64, f64) -> bool,
) -> Vec<(f64, f64)> {
    spans
        .iter()
        .filter_map(|&(a, b)| {
            let (a, b) = ((a + offset).max(0.0), (b + offset).min(duration));
            if b - a < LONG_PAUSE {
                return None;
            }
            let cut = (a + KEEP, b - KEEP);
            (cut.1 - cut.0 >= 0.3 && !busy(cut.0, cut.1)).then_some(cut)
        })
        .collect()
}

// Cleanup ------------------------------------------------------------------------------------------------------

/// A second-order Butterworth high-pass filter.
struct HighPass {
    b: [f32; 3],
    a: [f32; 2],
    x: [f32; 2],
    y: [f32; 2],
}

impl HighPass {
    fn new(rate: f32, cutoff: f32) -> Self {
        let w0 = std::f32::consts::TAU * cutoff / rate;
        let alpha = w0.sin() / std::f32::consts::SQRT_2;
        let cos = w0.cos();
        let a0 = 1.0 + alpha;
        Self {
            b: [(1.0 + cos) / 2.0 / a0, -(1.0 + cos) / a0, (1.0 + cos) / 2.0 / a0],
            a: [-2.0 * cos / a0, (1.0 - alpha) / a0],
            x: [0.0; 2],
            y: [0.0; 2],
        }
    }

    fn run(&mut self, samples: &mut [f32]) {
        for s in samples {
            let x = *s;
            let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1]
                - self.a[0] * self.y[0]
                - self.a[1] * self.y[1];
            self.x = [x, self.x[0]];
            self.y = [y, self.y[0]];
            *s = y;
        }
    }
}

/// Frames between updates of the noise estimate's history, and how many of those it remembers: about 1.4 s in
/// all, long enough to see past a sentence, short enough to follow a fan speeding up.
const STEPS: usize = 16;
const HISTORY: usize = 8;
/// The minimum of a noisy signal sits below its average; this lifts the estimate back up.
const BIAS: f32 = 1.6;
/// The most a frequency is turned down: about 14 dB, which keeps the voice natural.
const FLOOR_GAIN: f32 = 0.2;

/// Turns down steady background noise. Each frame's spectrum is compared with a running estimate of the noise (the
/// quietest each frequency has been lately), and frequencies are scaled by how much of them is voice. Gains follow
/// the "decision-directed" estimate of the voice's strength, which keeps the leftover noise smooth instead of
/// twittering.
struct Denoiser {
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    size: usize,
    hop: usize,
    window: Vec<f32>,
    /// Input waiting for a full frame. It starts with `size - hop` zeros, so the first frame lines up with time zero.
    pending: Vec<f32>,
    /// What the frames so far add to the next output.
    tail: Vec<f32>,
    frame: Vec<f32>,
    spectrum: Vec<Complex32>,
    smoothed: Vec<f32>,
    step_min: Vec<f32>,
    history: Vec<Vec<f32>>,
    steps: usize,
    voice: Vec<f32>,
    gains: Vec<f32>,
    started: bool,
    /// Output still to drop at the start, the frame's latency.
    skip: usize,
}

impl Denoiser {
    fn new(rate: u32) -> Self {
        // About 21 ms frames, half overlapping.
        let size = ((f64::from(rate) * 0.0213) as usize).next_power_of_two().max(256);
        let hop = size / 2;
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(size);
        let inverse = planner.plan_fft_inverse(size);
        let bins = size / 2 + 1;
        // A square-root Hann window on the way in and out: together a Hann window, whose half-overlapping copies
        // add up to exactly one, so a frame passed through unchanged comes out unchanged.
        let window = (0..size).map(|i| (std::f32::consts::PI * i as f32 / size as f32).sin()).collect();
        Self {
            spectrum: forward.make_output_vec(),
            frame: forward.make_input_vec(),
            forward,
            inverse,
            size,
            hop,
            window,
            pending: vec![0.0; size - hop],
            tail: vec![0.0; size - hop],
            smoothed: vec![0.0; bins],
            step_min: vec![f32::INFINITY; bins],
            history: Vec::with_capacity(HISTORY),
            steps: 0,
            voice: vec![0.0; bins],
            gains: vec![1.0; bins],
            started: false,
            skip: size - hop,
        }
    }

    fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        self.pending.extend_from_slice(input);
        let mut used = 0;
        while self.pending.len() - used >= self.size {
            self.process(used, out);
            used += self.hop;
        }
        self.pending.drain(..used);
    }

    fn process(&mut self, at: usize, out: &mut Vec<f32>) {
        let (size, hop) = (self.size, self.hop);
        for (i, f) in self.frame.iter_mut().enumerate() {
            *f = self.pending[at + i] * self.window[i];
        }
        if self.forward.process(&mut self.frame, &mut self.spectrum).is_err() {
            return;
        }

        // Track the noise: the smoothed power's minimum over the last second and a half.
        let bins = self.spectrum.len();
        for k in 0..bins {
            let power = self.spectrum[k].norm_sqr();
            self.smoothed[k] = if self.started { 0.8 * self.smoothed[k] + 0.2 * power } else { power };
            self.step_min[k] = self.step_min[k].min(self.smoothed[k]);
        }
        self.started = true;
        self.steps += 1;
        if self.steps == STEPS {
            if self.history.len() == HISTORY {
                self.history.remove(0);
            }
            self.history.push(std::mem::replace(&mut self.step_min, self.smoothed.clone()));
            self.steps = 0;
        }

        for k in 0..bins {
            let lowest = self.history.iter().fold(self.step_min[k], |m, h| m.min(h[k]));
            let noise = (lowest * BIAS).max(1e-12);
            let power = self.spectrum[k].norm_sqr();
            let after = power / noise;
            let before = 0.96 * self.voice[k] / noise + 0.04 * (after - 1.0).max(0.0);
            self.gains[k] = (before / (1.0 + before)).max(FLOOR_GAIN);
        }
        // A little smoothing across neighbouring frequencies keeps the leftover noise even.
        let mut previous = self.gains[0];
        for k in 0..bins {
            let next = self.gains.get(k + 1).copied().unwrap_or(self.gains[k]);
            let here = self.gains[k];
            let gain = 0.25 * previous + 0.5 * here + 0.25 * next;
            previous = here;
            self.spectrum[k] *= gain;
            self.voice[k] = self.spectrum[k].norm_sqr();
        }
        self.spectrum[0].im = 0.0;
        self.spectrum[bins - 1].im = 0.0;
        if self.inverse.process(&mut self.spectrum, &mut self.frame).is_err() {
            return;
        }

        // Overlap-add; the first half of this frame completes `hop` samples of output.
        let scale = 1.0 / size as f32;
        for i in 0..size {
            self.frame[i] *= self.window[i] * scale;
        }
        let mut ready: Vec<f32> = (0..hop).map(|i| self.tail[i] + self.frame[i]).collect();
        self.tail.copy_from_slice(&self.frame[hop..]);
        let drop = self.skip.min(ready.len());
        ready.drain(..drop);
        self.skip -= drop;
        out.extend_from_slice(&ready);
    }

    /// Pushes silence through so every input sample comes out.
    fn flush(&mut self, out: &mut Vec<f32>) {
        let zeros = vec![0.0; self.size];
        self.push(&zeros, out);
    }
}

/// Evens out loud and quiet passages: a slow gain toward a steady speaking level. It only adjusts while someone
/// speaks (well above the room's own noise) and holds still in pauses, so the room is never pumped up.
struct Leveller {
    power: f32,
    /// The room's noise level: follows the power straight down, and creeps up at 3 dB a second.
    floor: f32,
    gain: f32,
    follow: f32,
    ease: f32,
    rise: f32,
}

/// The level it aims for, about -20 dB below full scale on average while speaking.
const TARGET: f32 = 0.01;
/// Below this the leveller holds its gain: about -50 dB.
const GATE: f32 = 1e-5;

impl Leveller {
    fn new(rate: f32) -> Self {
        Self {
            power: GATE,
            floor: GATE,
            gain: 1.0,
            follow: (-1.0 / (0.4 * rate)).exp(),
            ease: (-1.0 / (0.6 * rate)).exp(),
            rise: 2f32.powf(1.0 / rate),
        }
    }

    fn run(&mut self, samples: &mut [f32]) {
        for s in samples {
            self.power = self.follow * self.power + (1.0 - self.follow) * *s * *s;
            self.floor = if self.power < self.floor { self.power.max(1e-9) } else { self.floor * self.rise };
            // Speaking: clearly louder than the room, about 9 dB above its noise.
            if self.power > GATE && self.power > 8.0 * self.floor {
                // At most 6 dB quieter or 9 dB louder: an even level, never a different voice.
                let wanted = (TARGET / self.power).sqrt().clamp(0.5, 2.8);
                self.gain = self.ease * self.gain + (1.0 - self.ease) * wanted;
            }
            *s = soft_limit(*s * self.gain);
        }
    }
}

/// Cleans up a voice as it streams through.
pub struct Cleaner {
    high_pass: HighPass,
    denoiser: Denoiser,
    leveller: Leveller,
    taken: usize,
    given: usize,
}

impl Cleaner {
    pub fn new(rate: u32) -> Self {
        let r = rate as f32;
        Self {
            high_pass: HighPass::new(r, 80.0),
            denoiser: Denoiser::new(rate),
            leveller: Leveller::new(r),
            taken: 0,
            given: 0,
        }
    }

    /// Cleans `input`, adding what's ready to `out`. The output trails the input by one frame until `finish`.
    pub fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let mut buffer = input.to_vec();
        self.high_pass.run(&mut buffer);
        let start = out.len();
        self.denoiser.push(&buffer, out);
        self.leveller.run(&mut out[start..]);
        self.taken += input.len();
        self.given += out.len() - start;
    }

    /// The rest of the output, which then matches the input sample for sample.
    pub fn finish(&mut self, out: &mut Vec<f32>) {
        let start = out.len();
        self.denoiser.flush(out);
        let missing = self.taken.saturating_sub(self.given);
        out.truncate(start + missing.min(out.len() - start));
        self.leveller.run(&mut out[start..]);
        self.given += out.len() - start;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, seconds: f64, frequency: f64, amplitude: f32) -> Vec<f32> {
        let n = (f64::from(rate) * seconds) as usize;
        (0..n)
            .map(|i| {
                amplitude * (std::f64::consts::TAU * frequency * i as f64 / f64::from(rate)).sin() as f32
            })
            .collect()
    }

    /// Deterministic white noise.
    fn hiss(n: usize, amplitude: f32) -> Vec<f32> {
        let mut state = 0x2545_f491_u32;
        (0..n)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                amplitude * (state as f32 / u32::MAX as f32 * 2.0 - 1.0)
            })
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|x| x * x).sum::<f32>() / samples.len().max(1) as f32).sqrt()
    }

    fn clean(input: &[f32], rate: u32) -> Vec<f32> {
        let mut cleaner = Cleaner::new(rate);
        let mut out = Vec::new();
        for chunk in input.chunks(4096) {
            cleaner.push(chunk, &mut out);
        }
        cleaner.finish(&mut out);
        out
    }

    #[test]
    fn the_limiter_leaves_normal_levels_alone_and_rounds_off_peaks() {
        assert_eq!(soft_limit(0.5), 0.5);
        assert_eq!(soft_limit(-0.9), -0.9);
        // Far above full scale it settles at full scale, never beyond.
        assert!(soft_limit(3.0) <= 1.0 && soft_limit(3.0) > 0.99);
        assert!(soft_limit(-3.0) >= -1.0);
        assert!(soft_limit(0.95) > 0.9 && soft_limit(0.95) < 0.95);
    }

    #[test]
    fn cleaning_keeps_the_length_and_the_timing() {
        let rate = 48_000;
        // A click at exactly one second must come out at exactly one second.
        let mut input = hiss(rate as usize * 2, 0.001);
        input[rate as usize] = 0.8;
        let out = clean(&input, rate);
        assert_eq!(out.len(), input.len());
        let peak =
            out.iter().enumerate().max_by(|a, b| a.1.abs().total_cmp(&b.1.abs())).map(|(i, _)| i).unwrap();
        assert!(peak.abs_diff(rate as usize) < 48, "the click moved to sample {peak}");
    }

    #[test]
    fn steady_hiss_goes_down_and_a_voice_stays() {
        let rate = 48_000;
        let n = rate as usize * 4;
        // Two seconds of hiss alone, then a "voice" (a 300 Hz tone) over the same hiss.
        let noise = hiss(n, 0.02);
        let voice = tone(rate, 4.0, 300.0, 0.1);
        let input: Vec<f32> = (0..n).map(|i| noise[i] + if i >= n / 2 { voice[i] } else { 0.0 }).collect();
        let out = clean(&input, rate);
        // Judged after the leveller settles, on the second second of hiss and the last second of voice.
        let quiet_before = rms(&input[rate as usize..2 * rate as usize]);
        let quiet_after = rms(&out[rate as usize..2 * rate as usize]);
        assert!(quiet_after < quiet_before * 0.5, "hiss {quiet_before} -> {quiet_after}");
        let voice_after = rms(&out[3 * rate as usize..]);
        assert!(voice_after > quiet_after * 8.0, "voice {voice_after} vs hiss {quiet_after}");
    }

    #[test]
    fn rumble_below_the_voice_is_filtered_out() {
        let rate = 48_000;
        let rumble = tone(rate, 2.0, 30.0, 0.3);
        let out = clean(&rumble, rate);
        assert!(rms(&out[rate as usize..]) < 0.3 * rms(&rumble[rate as usize..]));
    }

    #[test]
    fn pauses_are_found_between_words() {
        // 20 ms windows: speech at -20 dB, a 2 s pause at -60 dB, speech again.
        let mut levels = vec![-20.0; 100];
        levels.extend(vec![-60.0; 100]);
        levels.extend(vec![-20.0; 100]);
        let spans = quiet_spans(&levels);
        assert_eq!(spans.len(), 1);
        let (a, b) = spans[0];
        // Widened speech eats 0.1 s at each edge.
        assert!((a - 2.1).abs() < 0.03 && (b - 3.9).abs() < 0.03, "{a}..{b}");
        let cuts = pause_cuts(&spans, 0.5, 10.0, &|_, _| false);
        assert_eq!(cuts.len(), 1);
        assert!((cuts[0].0 - 2.8).abs() < 0.03 && (cuts[0].1 - 4.2).abs() < 0.03, "{:?}", cuts[0]);
        // Something happening on screen keeps the pause whole.
        assert!(pause_cuts(&spans, 0.5, 10.0, &|_, _| true).is_empty());
        // A take without a clear voice has nothing to cut.
        assert!(quiet_spans(&vec![-50.0; 300]).is_empty());
    }

    #[test]
    fn activity_counts_inputs_travel_and_animation() {
        let still = CursorSample { t_us: 0, x: 10.0, y: 10.0, ..CursorSample::default() };
        let moved = CursorSample { t_us: 1_000_000, x: 200.0, y: 10.0, ..CursorSample::default() };
        let frames: Vec<f64> = (0..60).map(|i| 5.0 + f64::from(i) / 30.0).collect();
        let a = Activity { inputs: &[3.0], cursor: &[still, moved], frames: &frames };
        assert!(a.busy(2.9, 3.5), "a click");
        assert!(a.busy(0.2, 0.8), "the pointer travelling");
        assert!(a.busy(5.0, 6.0), "the screen animating");
        assert!(!a.busy(8.0, 9.5), "nothing at all");
    }
}
