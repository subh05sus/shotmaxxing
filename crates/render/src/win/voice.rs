//! The voice tools on Windows: Media Foundation reads and writes the sound, and reads when the screen recording
//! stored its frames without decoding any of them.

use std::path::{Path, PathBuf};

use windows::Win32::Media::MediaFoundation::{
    IMFSinkWriter, MF_SOURCE_READER_ALL_STREAMS, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
    MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READERF_ERROR, MFCreateSourceReaderFromURL,
};
use windows::core::HSTRING;

use super::encode::{add_sound, sink, write_pcm};
use super::sound::AudioFile;
use super::{start, system};
use crate::RenderError;
use crate::mix::{RATE, Source};
use crate::voice::{self, MonoReader, MonoWriter, VoiceSystem};

/// Frames per buffer: about two thirds of a second.
const CHUNK: usize = 32_768;

/// A sound file read as one channel, a buffer at a time.
pub struct MonoFile {
    file: Box<dyn Source>,
    stereo: Vec<f32>,
    mono: Vec<f32>,
}

impl MonoReader for MonoFile {
    fn rate(&self) -> u32 {
        RATE
    }

    fn length(&self) -> u64 {
        (self.file.seconds() * f64::from(RATE)).round().max(1.0) as u64
    }

    fn next(&mut self) -> Result<Option<&[f32]>, RenderError> {
        self.stereo.resize(CHUNK * 2, 0.0);
        let read = self.file.read(&mut self.stereo)?;
        if read == 0 {
            return Ok(None);
        }
        self.mono.clear();
        self.mono.extend(self.stereo[..read * 2].as_chunks::<2>().0.iter().map(|[l, r]| (l + r) * 0.5));
        Ok(Some(&self.mono))
    }
}

/// A one-channel AAC file at 96 kbps, as the Mac writes it, filled a buffer at a time.
pub struct SoundWriter {
    writer: IMFSinkWriter,
    stream: u32,
    written: u64,
}

impl SoundWriter {
    fn create(path: &Path, rate: u32) -> Result<Self, RenderError> {
        if rate != RATE {
            return Err(RenderError::System(format!("sound at {rate} Hz can't be written here")));
        }
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let writer = sink(path)?;
        let stream = add_sound(&writer, 1, 96_000)?;
        // SAFETY: the writer has its stream and types set.
        unsafe { writer.BeginWriting() }.map_err(system)?;
        Ok(Self { writer, stream, written: 0 })
    }
}

impl MonoWriter for SoundWriter {
    fn write(&mut self, samples: &[f32]) -> Result<(), RenderError> {
        for chunk in samples.chunks(CHUNK) {
            write_pcm(&self.writer, self.stream, chunk, 1, self.written)?;
            self.written += chunk.len() as u64;
        }
        Ok(())
    }

    fn finish(self) -> Result<(), RenderError> {
        // SAFETY: finishes the file; the writer isn't used again.
        unsafe { self.writer.Finalize() }.map_err(|e| {
            RenderError::System(format!("the cleaned sound couldn't be finished: {}", e.message()))
        })
    }
}

/// When each frame of a video was stored, in seconds from the first. With no output format set, the source
/// reader hands the samples over as stored, so nothing is decoded and this costs next to nothing.
fn frame_times(path: &Path) -> Result<Vec<f64>, RenderError> {
    start()?;
    const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    let unreadable = |e: windows::core::Error| {
        RenderError::System(format!("couldn't read the recording: {}", e.message()))
    };
    // SAFETY: Media Foundation calls with valid objects; the out-pointers are valid for each call, and the path
    // outlives the call that reads it.
    unsafe {
        let reader = MFCreateSourceReaderFromURL(&HSTRING::from(path), None).map_err(unreadable)?;
        reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false).map_err(system)?;
        if reader.SetStreamSelection(VIDEO, true).is_err() {
            return Ok(Vec::new());
        }
        let mut times = Vec::new();
        loop {
            let (mut flags, mut timestamp, mut sample) = (0u32, 0i64, None);
            reader
                .ReadSample(
                    VIDEO,
                    0,
                    None,
                    Some(&raw mut flags),
                    Some(&raw mut timestamp),
                    Some(&raw mut sample),
                )
                .map_err(unreadable)?;
            if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 {
                return Err(RenderError::System("the recording couldn't be read to the end".into()));
            }
            if sample.is_some() {
                times.push(timestamp as f64 / 1e7);
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
        }
        times.sort_by(f64::total_cmp);
        if let Some(&base) = times.first() {
            times.iter_mut().for_each(|t| *t -= base);
        }
        Ok(times)
    }
}

/// Media Foundation's sound files and recordings, for the voice tools.
struct Windows;

impl VoiceSystem for Windows {
    type Reader = MonoFile;
    type Writer = SoundWriter;

    fn open(&self, path: &Path) -> Result<MonoFile, RenderError> {
        let file = AudioFile::open(path)?
            .ok_or_else(|| RenderError::System("couldn't open the recorded sound".into()))?;
        Ok(MonoFile { file, stereo: Vec::new(), mono: Vec::with_capacity(CHUNK) })
    }

    fn create(&self, path: &Path, rate: u32) -> Result<SoundWriter, RenderError> {
        SoundWriter::create(path, rate)
    }

    fn frame_times(&self, path: &Path) -> Result<Vec<f64>, RenderError> {
        frame_times(path)
    }
}

pub fn find_pauses(root: &Path) -> Result<Vec<(f64, f64)>, RenderError> {
    voice::find_pauses_with(&Windows, root)
}

pub fn clean_voice(root: &Path, progress: &mut dyn FnMut(f64) -> bool) -> Result<PathBuf, RenderError> {
    voice::clean_voice_with(&Windows, root, progress)
}

#[cfg(test)]
mod tests {
    use shotmaxxing_core::bundle::ProjectBundle;
    use shotmaxxing_core::geometry::Size;
    use shotmaxxing_core::project::{MediaTrack, Project, TrackKind};
    use tiny_skia::{Color, Pixmap};

    use super::super::encode::VideoWriter;
    use super::*;
    use crate::{CLEAN_MIC, Format};

    fn track(kind: TrackKind, path: &str) -> MediaTrack {
        MediaTrack {
            id: path.into(),
            kind,
            path: path.into(),
            duration_us: 6_000_000,
            offset_us: 0,
            volume: 1.0,
            muted: false,
            name: None,
        }
    }

    /// How loud `length` seconds of a sound file are from `from`, in dB below full scale.
    fn level(path: &Path, from: f64, length: f64) -> f64 {
        let mut reader = Windows.open(path).unwrap();
        let mut samples = Vec::new();
        while let Some(chunk) = reader.next().unwrap() {
            samples.extend_from_slice(chunk);
        }
        let at = |t: f64| ((t * f64::from(RATE)) as usize).min(samples.len());
        let slice = &samples[at(from)..at(from + length)];
        let power =
            slice.iter().map(|&x| f64::from(x) * f64::from(x)).sum::<f64>() / slice.len().max(1) as f64;
        10.0 * power.max(1e-12).log10()
    }

    #[test]
    fn long_pauses_are_found_and_the_voice_is_cleaned() {
        start().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut project = Project { name: "Voice test".into(), ..Project::default() };
        project.source.size = Size::new(640.0, 400.0);
        project.source.scale = 1.0;
        project.tracks = vec![
            track(TrackKind::Screen, "media/screen.mp4"),
            track(TrackKind::Microphone, "media/microphone.m4a"),
        ];
        let bundle = ProjectBundle::create(dir.path(), &mut project).unwrap();
        let media = bundle.media_dir();

        // A still screen stores a frame a second, like a recording of a screen where nothing moves.
        let Ok(mut screen) =
            VideoWriter::new(&media.join("screen.mp4"), 640, 400, 1, Format::H264, 1e6, false)
        else {
            eprintln!("skipped: this computer can't encode H.264");
            return;
        };
        let mut frame = Pixmap::new(640, 400).unwrap();
        frame.fill(Color::from_rgba8(51, 68, 85, 255));
        for _ in 0..6 {
            screen.write(&frame).unwrap();
        }
        screen.finish().unwrap();

        // A voice for half a second every two seconds, over a quiet room.
        let Ok(mut voice) = SoundWriter::create(&media.join("microphone.m4a"), RATE) else {
            eprintln!("skipped: this computer can't encode AAC");
            return;
        };
        let mut seed = 1u32;
        let samples: Vec<f32> = (0..6 * RATE as usize)
            .map(|i| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
                let speaking = (i as f64 / f64::from(RATE)) % 2.0 < 0.5;
                noise * if speaking { 0.5 } else { 0.015 }
            })
            .collect();
        voice.write(&samples).unwrap();
        voice.finish().unwrap();
        bundle.save(&mut project).unwrap();
        let root = bundle.root().to_path_buf();

        // Each 1.5 s gap loses all but a breath at each end, as on the Mac.
        let cuts = find_pauses(&root).unwrap();
        assert_eq!(cuts.len(), 3, "{cuts:?}");
        for (cut, expected) in cuts.iter().zip([(0.8, 1.7), (2.8, 3.7), (4.8, 5.8)]) {
            assert!(
                (cut.0 - expected.0).abs() < 0.08 && (cut.1 - expected.1).abs() < 0.08,
                "{cut:?} vs {expected:?}"
            );
        }

        let mut last = 0.0;
        let cleaned = clean_voice(&root, &mut |p| {
            assert!(p >= last);
            last = p;
            true
        })
        .unwrap();
        assert_eq!(cleaned, root.join(CLEAN_MIC));
        assert_eq!(last, 1.0);
        // Just as long, so it stays in sync with the picture.
        let original = media.join("microphone.m4a");
        let (before, after) = (super::super::audio_seconds(&original), super::super::audio_seconds(&cleaned));
        let (before, after) = (before.unwrap(), after.unwrap());
        assert!((before - after).abs() < 0.05, "{before} s became {after} s");
        // The room between words is clearly quieter, while the voice stays strong.
        let (room_before, room_after) = (level(&original, 1.0, 0.8), level(&cleaned, 1.0, 0.8));
        assert!(room_after < room_before - 6.0, "room {room_before} -> {room_after} dB");
        let (voice_before, voice_after) = (level(&original, 2.05, 0.4), level(&cleaned, 2.05, 0.4));
        assert!(voice_after > voice_before - 6.0, "voice {voice_before} -> {voice_after} dB");
    }
}
