//! An export's sound, mixed on the CPU: what the macOS exporter builds with AVFoundation, for systems without a
//! mixer of their own. It lays the sound out the same way:
//!
//! - every recorded sound plays through the cuts, sped up where the video is (keeping its pitch), with short fades
//!   at each cut so nothing clicks;
//! - music plays on the finished video's timeline and fades out with it when it runs past the end;
//! - click sounds sit under each press.
//!
//! Tracks set louder than 100% are mixed relative to the loudest, and the whole mix is raised by that much with a
//! soft limit, as on the Mac. The mix comes out in order, a piece at a time, as 48 kHz interleaved stereo.

use std::path::Path;

use crate::RenderError;
use crate::click::{CLICK_SECONDS, click_samples};
use crate::model::{RenderProject, Sound};
use crate::voice::soft_limit;

/// Frames per second of the mix and of the exported sound.
pub const RATE: u32 = 48_000;
/// How long each cut edge fades, in seconds.
const FADE: f64 = 0.008;
/// How long music fades out when the video ends before it does.
const MUSIC_FADE_OUT: f64 = 2.0;

/// Decoded sound, read in order as 48 kHz interleaved stereo.
pub trait Source {
    /// Fills `out` (whole frames) from where reading is, and says how many frames it wrote: 0 at the end.
    fn read(&mut self, out: &mut [f32]) -> Result<usize, RenderError>;
    /// Moves to `frame` frames from the start.
    fn seek(&mut self, frame: u64) -> Result<(), RenderError>;
    /// How long it plays, in seconds.
    fn seconds(&self) -> f64;
}

/// A sound file decoded, or `None` when it holds no sound.
pub type Opened = Result<Option<Box<dyn Source>>, RenderError>;

fn frames(seconds: f64) -> u64 {
    (seconds.max(0.0) * f64::from(RATE)).round() as u64
}

/// Where one stretch of a sound plays.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Segment {
    /// Where it starts in the export, and for how long, in frames.
    at: u64,
    length: u64,
    /// Where it starts in the sound, in frames.
    from: u64,
    /// How much faster than recorded it plays.
    speed: f64,
    level: f32,
    fade_in: u64,
    fade_out: u64,
}

/// Where `sound`, `seconds` long, plays in the export, at `level`.
fn plan(project: &RenderProject, sound: &Sound, seconds: f64, level: f32) -> Vec<Segment> {
    let total = project.output_duration();
    if sound.continuous {
        // Music plays on the finished video's timeline, untouched by cuts and speed changes: in without a click, and
        // out with the video when it runs past the end.
        let start = sound.offset.max(0.0);
        let length = seconds.min(total - start);
        if length <= 0.002 {
            return Vec::new();
        }
        let fade = if start + seconds > total + 1e-3 { MUSIC_FADE_OUT.min(length / 2.0) } else { FADE };
        return vec![Segment {
            at: frames(start),
            length: frames(length),
            from: 0,
            speed: 1.0,
            level,
            fade_in: frames(FADE),
            fade_out: frames(fade).max(1),
        }];
    }
    // `out` is where the clip starts in the export.
    let mut out = 0.0;
    let mut segments = Vec::new();
    for clip in &project.clips {
        // The clip in the sound's own time, and the part of it the sound covers.
        let (s0, s1) = (clip.start - sound.offset, clip.end - sound.offset);
        let (f0, f1) = (s0.max(0.0), s1.min(seconds));
        if f1 - f0 > 0.002 {
            let length = (f1 - f0) / clip.speed;
            let fade = if length > 4.0 * FADE { frames(FADE) } else { 0 };
            segments.push(Segment {
                at: frames(out + (f0 - s0) / clip.speed),
                length: frames(length),
                from: frames(f0),
                speed: clip.speed,
                level,
                fade_in: fade,
                fade_out: fade,
            });
        }
        out += (clip.end - clip.start) / clip.speed;
    }
    segments
}

/// One sound, playing its segments in order.
struct Voice {
    source: Box<dyn Source>,
    segments: Vec<Segment>,
    /// The first segment that hasn't finished.
    next: usize,
    /// Where reading is in the source, in frames.
    position: u64,
    /// The time-stretch for a sped-up segment, by its index.
    stretch: Option<(usize, Stretch)>,
    scratch: Vec<f32>,
}

impl Voice {
    /// Reads `count` frames from `frame` of the source into the scratch buffer, with silence past its end.
    fn read_at(&mut self, frame: u64, count: usize) -> Result<(), RenderError> {
        if frame != self.position {
            if frame > self.position && frame - self.position < u64::from(RATE) {
                // Reading on is cheaper than seeking for a short way.
                let mut skip = vec![0f32; ((frame - self.position) * 2) as usize];
                let read = fill(self.source.as_mut(), &mut skip)?;
                self.position += read as u64;
            }
            if frame != self.position {
                self.source.seek(frame)?;
                self.position = frame;
            }
        }
        self.scratch.clear();
        self.scratch.resize(count * 2, 0.0);
        let read = fill(self.source.as_mut(), &mut self.scratch)?;
        self.position += read as u64;
        Ok(())
    }

    /// Adds this sound's part of the export's frames `start..start + out.len() / 2` into `out`.
    fn mix_into(&mut self, start: u64, out: &mut [f32]) -> Result<(), RenderError> {
        let end = start + (out.len() / 2) as u64;
        while let Some(&segment) = self.segments.get(self.next) {
            if segment.at >= end {
                break;
            }
            let finish = segment.at + segment.length;
            let (a, b) = (segment.at.max(start), finish.min(end));
            if a < b {
                let (offset, count) = (a - segment.at, (b - a) as usize);
                if (segment.speed - 1.0).abs() < 1e-3 {
                    self.read_at(segment.from + offset, count)?;
                } else {
                    if self.stretch.as_ref().is_none_or(|(i, _)| *i != self.next) {
                        self.source.seek(segment.from)?;
                        self.position = segment.from;
                        self.stretch = Some((self.next, Stretch::new(segment.speed)));
                    }
                    if let Some((_, stretch)) = &mut self.stretch {
                        self.scratch.clear();
                        self.scratch.resize(count * 2, 0.0);
                        stretch.produce(self.source.as_mut(), &mut self.scratch)?;
                    }
                    // Where reading is now belongs to the stretch; the next plain read seeks.
                    self.position = u64::MAX;
                }
                for k in 0..count {
                    let frame = offset + k as u64;
                    let mut gain = segment.level;
                    if frame < segment.fade_in {
                        gain *= frame as f32 / segment.fade_in as f32;
                    }
                    let left = segment.length - frame;
                    if left <= segment.fade_out {
                        gain *= (left - 1) as f32 / segment.fade_out as f32;
                    }
                    let at = ((a - start) as usize + k) * 2;
                    out[at] += self.scratch[2 * k] * gain;
                    out[at + 1] += self.scratch[2 * k + 1] * gain;
                }
            }
            if finish > end {
                break;
            }
            self.next += 1;
            self.stretch = None;
        }
        Ok(())
    }
}

/// Reads until `out` is full or the source ends, and says how many frames it read.
fn fill(source: &mut dyn Source, out: &mut [f32]) -> Result<usize, RenderError> {
    let mut done = 0;
    while done * 2 < out.len() {
        let read = source.read(&mut out[done * 2..])?;
        if read == 0 {
            break;
        }
        done += read;
    }
    Ok(done)
}

/// Plays sound faster or slower while keeping its pitch (WSOLA): overlapping windows of the source, each taken
/// where it best continues the last, are laid one half-window apart.
struct Stretch {
    speed: f64,
    /// Source frames read and still needed, interleaved stereo; the first is source frame `base` of the segment.
    input: Vec<f32>,
    base: u64,
    exhausted: bool,
    /// Where the last window started in the segment's source, if there was one.
    last: Option<u64>,
    /// Windows laid so far.
    steps: u64,
    /// The second half of the last window, waiting for the next one to overlap it.
    tail: Vec<f32>,
    /// Finished frames not handed out yet.
    ready: Vec<f32>,
    window: Vec<f32>,
}

/// A window is about 21 ms: long enough for a voice's pitch, short enough to follow it.
const WINDOW: usize = 1024;
const HOP: usize = WINDOW / 2;
/// How far, in frames, a window may move to line up with the last one.
const SEARCH: usize = 192;

impl Stretch {
    fn new(speed: f64) -> Self {
        // A periodic Hann window: two at half a window apart add up to one.
        let window = (0..WINDOW)
            .map(|n| (0.5 - 0.5 * (2.0 * std::f64::consts::PI * n as f64 / WINDOW as f64).cos()) as f32)
            .collect();
        Self {
            speed,
            input: Vec::new(),
            base: 0,
            exhausted: false,
            last: None,
            steps: 0,
            tail: vec![0.0; HOP * 2],
            ready: Vec::new(),
            window,
        }
    }

    /// Makes sure source frames up to `end` (of the segment) are read, as far as the source goes.
    fn read_to(&mut self, source: &mut dyn Source, end: u64) -> Result<(), RenderError> {
        while !self.exhausted && self.base + (self.input.len() / 2) as u64 <= end {
            let have = self.input.len();
            self.input.resize(have + 4096 * 2, 0.0);
            let read = source.read(&mut self.input[have..])?;
            self.input.truncate(have + read * 2);
            self.exhausted = read == 0;
        }
        Ok(())
    }

    /// Source frame `frame` of the segment, as a mono sum; silence outside what's read.
    fn mono(&self, frame: u64) -> f32 {
        let Some(i) = frame.checked_sub(self.base).map(|i| i as usize * 2) else { return 0.0 };
        match (self.input.get(i), self.input.get(i + 1)) {
            (Some(l), Some(r)) => l + r,
            _ => 0.0,
        }
    }

    fn sample(&self, frame: u64, channel: usize) -> f32 {
        frame
            .checked_sub(self.base)
            .and_then(|i| self.input.get(i as usize * 2 + channel))
            .copied()
            .unwrap_or(0.0)
    }

    /// Lays the next window, which finishes half a window of output.
    fn step(&mut self, source: &mut dyn Source) -> Result<(), RenderError> {
        let nominal = (self.steps as f64 * HOP as f64 * self.speed).round() as u64;
        let start = match self.last {
            None => nominal,
            Some(last) => {
                // The window that best continues the last one's second half, near where it should be.
                let natural = last + HOP as u64;
                let low = nominal.saturating_sub(SEARCH as u64);
                self.read_to(source, nominal + (SEARCH + WINDOW) as u64)?;
                let mut best = (f32::MIN, nominal);
                for candidate in low..=nominal + SEARCH as u64 {
                    let mut score = 0.0;
                    for n in (0..HOP as u64).step_by(2) {
                        score += self.mono(candidate + n) * self.mono(natural + n);
                    }
                    if score > best.0 {
                        best = (score, candidate);
                    }
                }
                best.1
            }
        };
        self.read_to(source, start + WINDOW as u64)?;
        self.ready.clear();
        for n in 0..HOP {
            let (w, frame) = (self.window[n], start + n as u64);
            for c in 0..2 {
                self.ready.push(self.tail[n * 2 + c] + self.sample(frame, c) * w);
            }
        }
        for n in 0..HOP {
            let (w, frame) = (self.window[HOP + n], start + (HOP + n) as u64);
            for c in 0..2 {
                self.tail[n * 2 + c] = self.sample(frame, c) * w;
            }
        }
        self.last = Some(start);
        self.steps += 1;
        // What's behind the next search can go.
        let next = (self.steps as f64 * HOP as f64 * self.speed).round() as u64;
        let keep = next.saturating_sub(SEARCH as u64).min(start + HOP as u64);
        if keep > self.base {
            let drop = ((keep - self.base) as usize * 2).min(self.input.len());
            self.input.drain(..drop);
            self.base += (drop / 2) as u64;
        }
        Ok(())
    }

    /// Fills `out` with the next frames of stretched sound.
    fn produce(&mut self, source: &mut dyn Source, out: &mut [f32]) -> Result<(), RenderError> {
        let mut done = 0;
        while done < out.len() {
            if self.ready.is_empty() {
                self.step(source)?;
            }
            let take = self.ready.len().min(out.len() - done);
            out[done..done + take].copy_from_slice(&self.ready[..take]);
            self.ready.drain(..take);
            done += take;
        }
        Ok(())
    }
}

pub struct Mixer {
    voices: Vec<Voice>,
    /// Where each click sounds in the export, and for how long, in frames.
    clicks: Vec<(u64, u64)>,
    click: Vec<f32>,
    click_level: f32,
    gain: f32,
    total: u64,
    done: u64,
    chunk: Vec<f32>,
}

impl Mixer {
    /// The project's sound, ready to mix in order; `None` when there's nothing to hear. `open` decodes a sound
    /// file, or says it holds no sound.
    pub fn new(
        project: &RenderProject,
        open: &mut dyn FnMut(&Path) -> Opened,
    ) -> Result<Option<Self>, RenderError> {
        let clicks = project.click_times();
        if project.sounds.is_empty() && clicks.is_empty() {
            return Ok(None);
        }
        let loudest = project.sounds.iter().map(|s| s.volume).fold(1.0, f64::max);
        let total = frames(project.output_duration());
        let mut voices = Vec::new();
        for sound in &project.sounds {
            let Some(source) = open(&sound.path)? else { continue };
            let segments = plan(project, sound, source.seconds(), (sound.volume / loudest) as f32);
            if !segments.is_empty() {
                voices.push(Voice {
                    source,
                    segments,
                    next: 0,
                    position: 0,
                    stretch: None,
                    scratch: Vec::new(),
                });
            }
        }
        // A click still sounding when the next press comes stands for both.
        let click = click_samples(RATE);
        let length = frames(CLICK_SECONDS).min(click.len() as u64);
        let mut laid: Vec<(u64, u64)> = Vec::new();
        for at in clicks.into_iter().map(frames) {
            let room = length.min(total.saturating_sub(at));
            if room < frames(0.002) || laid.last().is_some_and(|(a, l)| at < a + l) {
                continue;
            }
            laid.push((at, room));
        }
        if voices.is_empty() && laid.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            voices,
            clicks: laid,
            click,
            click_level: (1.0 / loudest) as f32,
            gain: loudest as f32,
            total,
            done: 0,
            chunk: Vec::new(),
        }))
    }

    /// The export's length, in frames.
    #[cfg(test)]
    fn total(&self) -> u64 {
        self.total
    }

    /// The next `count` frames of the mix (fewer at the end), interleaved stereo; `None` once all of it is out.
    pub fn next(&mut self, count: usize) -> Result<Option<&[f32]>, RenderError> {
        let count = (count as u64).min(self.total - self.done) as usize;
        if count == 0 {
            return Ok(None);
        }
        let start = self.done;
        self.chunk.clear();
        self.chunk.resize(count * 2, 0.0);
        for voice in &mut self.voices {
            voice.mix_into(start, &mut self.chunk)?;
        }
        let end = start + count as u64;
        for &(at, length) in &self.clicks {
            if at >= end {
                break;
            }
            for frame in at.max(start)..(at + length).min(end) {
                let sample = self.click[(frame - at) as usize] * self.click_level;
                let i = (frame - start) as usize * 2;
                self.chunk[i] += sample;
                self.chunk[i + 1] += sample;
            }
        }
        for x in &mut self.chunk {
            *x = soft_limit(*x * self.gain);
        }
        self.done = end;
        Ok(Some(&self.chunk))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::model::Clip;

    /// A tone: `hz` at `amplitude` in both channels for `seconds`.
    struct Tone {
        hz: f64,
        amplitude: f32,
        length: u64,
        at: u64,
    }

    impl Tone {
        fn new(hz: f64, amplitude: f32, seconds: f64) -> Self {
            Self { hz, amplitude, length: frames(seconds), at: 0 }
        }

        fn value(&self, frame: u64) -> f32 {
            self.amplitude
                * (2.0 * std::f64::consts::PI * self.hz * frame as f64 / f64::from(RATE)).sin() as f32
        }
    }

    impl Source for Tone {
        fn read(&mut self, out: &mut [f32]) -> Result<usize, RenderError> {
            let count = ((out.len() / 2) as u64).min(self.length - self.at.min(self.length)) as usize;
            for k in 0..count {
                let v = self.value(self.at + k as u64);
                out[2 * k] = v;
                out[2 * k + 1] = v;
            }
            self.at += count as u64;
            Ok(count)
        }

        fn seek(&mut self, frame: u64) -> Result<(), RenderError> {
            self.at = frame;
            Ok(())
        }

        fn seconds(&self) -> f64 {
            self.length as f64 / f64::from(RATE)
        }
    }

    fn project(clips: Vec<Clip>, sounds: Vec<Sound>) -> RenderProject {
        let blank = shotmaxxing_core::project::Project::default();
        let mut project = RenderProject::from_parts(Path::new("/nowhere"), &blank, &[], &[]);
        project.clips = clips;
        project.sounds = sounds;
        project.click_sounds = false;
        project.clicks = Vec::new();
        project
    }

    fn sound(path: &str, offset: f64, volume: f64, continuous: bool) -> Sound {
        Sound { path: PathBuf::from(path), offset, volume, continuous }
    }

    fn clip(start: f64, end: f64, speed: f64) -> Clip {
        Clip { start, end, speed }
    }

    fn mix_all(mixer: &mut Mixer) -> Vec<f32> {
        let mut all = Vec::new();
        while let Some(chunk) = mixer.next(1000).unwrap() {
            all.extend_from_slice(chunk);
        }
        all
    }

    fn rms(pcm: &[f32], from: f64, to: f64) -> f32 {
        let (a, b) = (frames(from) as usize * 2, (frames(to) as usize * 2).min(pcm.len()));
        let slice = &pcm[a..b];
        (slice.iter().map(|x| x * x).sum::<f32>() / slice.len().max(1) as f32).sqrt()
    }

    #[test]
    fn sound_follows_the_cuts_with_fades_at_each_edge() {
        // Kept: 0–1 s and 2–3 s; the microphone started half a second before the screen.
        let p = project(vec![clip(0.0, 1.0, 1.0), clip(2.0, 3.0, 1.0)], vec![sound("mic", -0.5, 1.0, false)]);
        let segments = plan(&p, &p.sounds[0], 4.0, 1.0);
        assert_eq!(segments.len(), 2);
        assert_eq!((segments[0].at, segments[0].from, segments[0].length), (0, frames(0.5), frames(1.0)));
        assert_eq!(
            (segments[1].at, segments[1].from, segments[1].length),
            (frames(1.0), frames(2.5), frames(1.0))
        );
        assert_eq!(segments[0].fade_in, frames(FADE));

        let mut open = |_: &Path| -> Opened { Ok(Some(Box::new(Tone::new(440.0, 0.5, 4.0)))) };
        let mut mixer = Mixer::new(&p, &mut open).unwrap().unwrap();
        assert_eq!(mixer.total(), frames(2.0));
        let pcm = mix_all(&mut mixer);
        assert_eq!(pcm.len(), frames(2.0) as usize * 2);
        // The tone plays at its level in both halves, and fades to nothing at the start and the cut.
        let tone = 0.5 / 2f32.sqrt();
        assert!((rms(&pcm, 0.1, 0.9) - tone).abs() < 0.01, "{}", rms(&pcm, 0.1, 0.9));
        assert!((rms(&pcm, 1.1, 1.9) - tone).abs() < 0.01);
        assert!(pcm[0].abs() < 1e-6);
        let cut = frames(1.0) as usize * 2;
        assert!(pcm[cut - 2].abs() < 1e-3 && pcm[cut].abs() < 1e-3);
        // The second half continues the source from 2.5 s, not from where the first stopped.
        let tone = Tone::new(440.0, 0.5, 4.0);
        let expected = tone.value(frames(2.5) + frames(0.5));
        let got = pcm[(frames(1.5) as usize) * 2];
        assert!((got - expected).abs() < 1e-3, "{got} vs {expected}");
    }

    #[test]
    fn music_plays_through_cuts_and_fades_out_with_the_video() {
        let p = project(vec![clip(0.0, 1.0, 1.0), clip(2.0, 3.0, 1.0)], vec![sound("song", 0.0, 0.8, true)]);
        let mut open = |_: &Path| -> Opened { Ok(Some(Box::new(Tone::new(220.0, 0.5, 10.0)))) };
        let segments = plan(&p, &p.sounds[0], 10.0, 1.0);
        assert_eq!(
            segments,
            vec![Segment {
                at: 0,
                length: frames(2.0),
                from: 0,
                speed: 1.0,
                level: 1.0,
                fade_in: frames(FADE),
                fade_out: frames(1.0)
            }]
        );
        let mut mixer = Mixer::new(&p, &mut open).unwrap().unwrap();
        let pcm = mix_all(&mut mixer);
        // Through the cut without a break, then quieter and quieter to the end.
        let full = rms(&pcm, 0.2, 0.8);
        assert!(full > 0.25, "{full}");
        assert!((rms(&pcm, 0.95, 1.05) - full).abs() < 0.03);
        assert!(rms(&pcm, 1.7, 1.8) < full * 0.35);
        assert!(pcm[pcm.len() - 2].abs() < 1e-3);
    }

    #[test]
    fn sped_up_sound_keeps_its_pitch_and_length() {
        let p = project(vec![clip(0.0, 2.0, 2.0)], vec![sound("mic", 0.0, 1.0, false)]);
        let mut open = |_: &Path| -> Opened { Ok(Some(Box::new(Tone::new(440.0, 0.5, 2.0)))) };
        let mut mixer = Mixer::new(&p, &mut open).unwrap().unwrap();
        let pcm = mix_all(&mut mixer);
        assert_eq!(pcm.len(), frames(1.0) as usize * 2);
        // Still 440 Hz: count the rising zero crossings in the middle half second.
        let left: Vec<f32> = pcm.iter().step_by(2).copied().collect();
        let (a, b) = (frames(0.25) as usize, frames(0.75) as usize);
        let rising = left[a..b].windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        assert!((215..=225).contains(&rising), "{rising} rising crossings in half a second");
        // And as loud as it was, without dips where windows meet.
        let tone = 0.5 / 2f32.sqrt();
        for k in 0..8 {
            let from = 0.1 + 0.1 * f64::from(k);
            let level = rms(&pcm, from, from + 0.05);
            assert!((level - tone).abs() < 0.04, "{level} at {from} s");
        }
    }

    #[test]
    fn louder_tracks_raise_the_whole_mix_and_clicks_sit_under_presses() {
        let mut p = project(vec![clip(0.0, 1.0, 1.0)], vec![sound("mic", 0.0, 1.5, false)]);
        p.click_sounds = true;
        p.clicks = vec![(0.5, shotmaxxing_core::geometry::Point::new(0.0, 0.0))];
        let mut open = |_: &Path| -> Opened { Ok(Some(Box::new(Tone::new(440.0, 0.2, 1.0)))) };
        let mut mixer = Mixer::new(&p, &mut open).unwrap().unwrap();
        assert_eq!(mixer.clicks, vec![(frames(0.5), frames(CLICK_SECONDS))]);
        let pcm = mix_all(&mut mixer);
        // 150% of a 0.2 tone.
        let level = rms(&pcm, 0.1, 0.4);
        assert!((level - 0.3 / 2f32.sqrt()).abs() < 0.01, "{level}");
        assert!(rms(&pcm, 0.5, 0.52) > rms(&pcm, 0.6, 0.62) + 0.01, "the click is missing");
    }

    #[test]
    fn nothing_to_hear_is_no_sound_at_all() {
        let p = project(vec![clip(0.0, 1.0, 1.0)], vec![sound("missing", 0.0, 1.0, false)]);
        let mut open = |_: &Path| -> Opened { Ok(None) };
        assert!(Mixer::new(&p, &mut open).unwrap().is_none());
        let p = project(vec![clip(0.0, 1.0, 1.0)], Vec::new());
        assert!(Mixer::new(&p, &mut open).unwrap().is_none());
    }
}
