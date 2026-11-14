//! The voice tools on macOS: sound files read and written a buffer at a time with AVAudioFile, and the screen
//! recording's frame times read without decoding a single frame.

use std::path::{Path, PathBuf};

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::AnyObject;
use objc2::{AllocAnyThread, msg_send, sel};
use objc2_av_foundation::{AVAssetReader, AVAssetReaderTrackOutput, AVMediaTypeVideo, AVURLAsset};
use objc2_avf_audio::{AVAudioCommonFormat, AVAudioFile, AVAudioPCMBuffer};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL, ns_string};
use shotmaxxing_core::bundle::ProjectBundle;
use shotmaxxing_core::project::{Project, TrackKind};

use super::decode::system;
use crate::voice::{Activity, Cleaner, Meter, pause_cuts, quiet_spans};
use crate::{CLEAN_MIC, RenderError};

/// Frames per buffer: about two thirds of a second at 48 kHz.
const CHUNK: u32 = 32_768;

fn url(path: &Path) -> Result<Retained<NSURL>, RenderError> {
    NSURL::from_file_path(path).ok_or_else(|| system(format!("not a valid file path: {}", path.display())))
}

/// A sound file read as one channel of floats, a buffer at a time.
struct AudioReader {
    file: Retained<AVAudioFile>,
    buffer: Retained<AVAudioPCMBuffer>,
    channels: usize,
    rate: u32,
    mono: Vec<f32>,
}

impl AudioReader {
    fn open(path: &Path) -> Result<Self, RenderError> {
        let url = url(path)?;
        // SAFETY: plain AVFAudio calls with valid arguments; the objects live as long as the reader.
        unsafe {
            let file = AVAudioFile::initForReading_error(AVAudioFile::alloc(), &url).map_err(|e| {
                system(format!("couldn't open the recorded sound: {}", e.localizedDescription()))
            })?;
            let format = file.processingFormat();
            let buffer =
                AVAudioPCMBuffer::initWithPCMFormat_frameCapacity(AVAudioPCMBuffer::alloc(), &format, CHUNK)
                    .ok_or_else(|| system("out of memory for the sound"))?;
            Ok(Self {
                channels: format.channelCount().max(1) as usize,
                rate: format.sampleRate().round().max(1.0) as u32,
                file,
                buffer,
                mono: Vec::with_capacity(CHUNK as usize),
            })
        }
    }

    /// Frames in the file.
    fn length(&self) -> u64 {
        // SAFETY: reading the file's length is a pure query.
        unsafe { self.file.length() }.max(1) as u64
    }

    /// The next buffer of sound, mixed down to one channel; `None` at the end.
    fn next(&mut self) -> Result<Option<&[f32]>, RenderError> {
        // SAFETY: reading into a buffer made for this file's format; the channel pointers are valid for
        // `frameLength` floats each until the next read.
        unsafe {
            if self.file.framePosition() >= self.file.length() {
                return Ok(None);
            }
            self.file
                .readIntoBuffer_error(&self.buffer)
                .map_err(|e| system(format!("couldn't read the sound: {}", e.localizedDescription())))?;
            let frames = self.buffer.frameLength() as usize;
            if frames == 0 {
                return Ok(None);
            }
            let data = self.buffer.floatChannelData();
            if data.is_null() {
                return Err(system("the sound isn't in a readable format"));
            }
            self.mono.clear();
            self.mono.resize(frames, 0.0);
            for c in 0..self.channels {
                let channel = std::slice::from_raw_parts((*data.add(c)).as_ptr(), frames);
                for (m, x) in self.mono.iter_mut().zip(channel) {
                    *m += x;
                }
            }
            if self.channels > 1 {
                let share = 1.0 / self.channels as f32;
                self.mono.iter_mut().for_each(|m| *m *= share);
            }
            Ok(Some(&self.mono))
        }
    }
}

/// A one-channel AAC file written a buffer at a time.
struct AudioWriter {
    file: Option<Retained<AVAudioFile>>,
    buffer: Retained<AVAudioPCMBuffer>,
}

impl AudioWriter {
    fn create(path: &Path, rate: u32) -> Result<Self, RenderError> {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        let url = url(path)?;
        let aac = NSNumber::new_u32(u32::from_be_bytes(*b"aac "));
        let rate = NSNumber::new_f64(f64::from(rate));
        let one = NSNumber::new_u32(1);
        let bitrate = NSNumber::new_u32(96_000);
        let settings = NSDictionary::<NSString, AnyObject>::from_slices(
            &[
                ns_string!("AVFormatIDKey"),
                ns_string!("AVSampleRateKey"),
                ns_string!("AVNumberOfChannelsKey"),
                ns_string!("AVEncoderBitRateKey"),
            ],
            &[aac.as_ref(), rate.as_ref(), one.as_ref(), bitrate.as_ref()],
        );
        // SAFETY: plain AVFAudio calls with valid arguments.
        unsafe {
            let file = AVAudioFile::initForWriting_settings_commonFormat_interleaved_error(
                AVAudioFile::alloc(),
                &url,
                &settings,
                AVAudioCommonFormat::PCMFormatFloat32,
                false,
            )
            .map_err(|e| system(format!("couldn't write the cleaned sound: {}", e.localizedDescription())))?;
            let buffer = AVAudioPCMBuffer::initWithPCMFormat_frameCapacity(
                AVAudioPCMBuffer::alloc(),
                &file.processingFormat(),
                CHUNK,
            )
            .ok_or_else(|| system("out of memory for the sound"))?;
            Ok(Self { file: Some(file), buffer })
        }
    }

    fn write(&mut self, samples: &[f32]) -> Result<(), RenderError> {
        let Some(file) = &self.file else { return Ok(()) };
        for chunk in samples.chunks(CHUNK as usize) {
            // SAFETY: the buffer holds `CHUNK` floats in its one channel, and `chunk` is no longer than that.
            unsafe {
                let data = self.buffer.floatChannelData();
                std::ptr::copy_nonoverlapping(chunk.as_ptr(), (*data).as_ptr(), chunk.len());
                self.buffer.setFrameLength(chunk.len() as u32);
                file.writeFromBuffer_error(&self.buffer).map_err(|e| {
                    system(format!("couldn't write the cleaned sound: {}", e.localizedDescription()))
                })?;
            }
        }
        Ok(())
    }

    /// Closes the file, which writes its index, so it's complete on disk.
    fn finish(mut self) {
        if let Some(file) = self.file.take() {
            // SAFETY: `close` exists from macOS 15; earlier, releasing the last reference closes the file.
            unsafe {
                let closes: bool = msg_send![&file, respondsToSelector: sel!(close)];
                if closes {
                    let _: () = msg_send![&file, close];
                }
            }
        }
    }
}

/// When each frame of a video was stored, in seconds from the first. The samples are read still compressed, so
/// this costs next to nothing.
fn frame_times(path: &Path) -> Result<Vec<f64>, RenderError> {
    let url = url(path)?;
    // SAFETY: plain AVFoundation calls with valid arguments, on objects that live for this call.
    unsafe {
        let asset = AVURLAsset::URLAssetWithURL_options(&url, None);
        let video = AVMediaTypeVideo.ok_or_else(|| system("AVFoundation is missing AVMediaTypeVideo"))?;
        #[allow(deprecated)]
        let Some(track) = asset.tracksWithMediaType(video).firstObject() else { return Ok(Vec::new()) };
        let reader = AVAssetReader::assetReaderWithAsset_error(&asset)
            .map_err(|e| system(format!("couldn't read the recording: {}", e.localizedDescription())))?;
        let output = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(&track, None);
        output.setAlwaysCopiesSampleData(false);
        if !reader.canAddOutput(&output) {
            return Err(system("the recording can't be read"));
        }
        reader.addOutput(&output);
        if !reader.startReading() {
            return Err(system("couldn't start reading the recording"));
        }
        let mut times = Vec::new();
        while let Some(sample) = autoreleasepool(|_| output.copyNextSampleBuffer()) {
            let t = sample.presentation_time_stamp().seconds();
            if sample.num_samples() > 0 && t.is_finite() {
                times.push(t);
            }
        }
        times.sort_by(f64::total_cmp);
        if let Some(&base) = times.first() {
            times.iter_mut().for_each(|t| *t -= base);
        }
        Ok(times)
    }
}

fn open(root: &Path) -> Result<(PathBuf, Project), RenderError> {
    let (bundle, project) = ProjectBundle::open(root).map_err(|e| RenderError::Project(e.to_string()))?;
    Ok((bundle.root().to_path_buf(), project))
}

pub fn find_pauses(root: &Path) -> Result<Vec<(f64, f64)>, RenderError> {
    let (root, project) = open(root)?;
    let Some(mic) = project.tracks.iter().find(|t| t.kind == TrackKind::Microphone) else {
        return Ok(Vec::new());
    };
    let spans = autoreleasepool(|_| -> Result<_, RenderError> {
        let mut reader = AudioReader::open(&root.join(&mic.path))?;
        let mut meter = Meter::new(reader.rate);
        while let Some(chunk) = reader.next()? {
            meter.push(chunk);
        }
        Ok(quiet_spans(&meter.finish()))
    })?;
    if spans.is_empty() {
        return Ok(Vec::new());
    }
    let (cursor, events) = crate::model::read_telemetry(&root);
    let inputs: Vec<f64> = events.iter().map(|e| e.t_us() as f64 / 1e6).collect();
    let frames = match project.tracks.iter().find(|t| t.kind == TrackKind::Screen) {
        Some(screen) => frame_times(&root.join(&screen.path))?,
        None => Vec::new(),
    };
    let activity = Activity { inputs: &inputs, cursor: &cursor, frames: &frames };
    let duration = project.duration_us() as f64 / 1e6;
    Ok(pause_cuts(&spans, mic.offset_us as f64 / 1e6, duration, &|a, b| activity.busy(a, b)))
}

pub fn clean_voice(root: &Path, progress: &mut dyn FnMut(f64) -> bool) -> Result<PathBuf, RenderError> {
    let (root, project) = open(root)?;
    let mic = project
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Microphone)
        .ok_or_else(|| RenderError::Project("this take was recorded without a microphone".into()))?;
    let target = root.join(CLEAN_MIC);
    // Written under a hidden name and renamed when complete, so a half-written file is never played.
    let temp = target.with_file_name(".microphone-clean.partial.m4a");
    let result = autoreleasepool(|_| -> Result<(), RenderError> {
        let mut reader = AudioReader::open(&root.join(&mic.path))?;
        let total = reader.length() as f64;
        let mut writer = AudioWriter::create(&temp, reader.rate)?;
        let mut cleaner = Cleaner::new(reader.rate);
        let (mut out, mut done) = (Vec::new(), 0usize);
        while let Some(chunk) = reader.next()? {
            done += chunk.len();
            cleaner.push(chunk, &mut out);
            writer.write(&out)?;
            out.clear();
            if !progress((done as f64 / total).min(0.99)) {
                return Err(RenderError::Cancelled);
            }
        }
        cleaner.finish(&mut out);
        writer.write(&out)?;
        writer.finish();
        Ok(())
    });
    match result {
        Ok(()) => {
            std::fs::rename(&temp, &target)?;
            progress(1.0);
            Ok(target)
        }
        Err(error) => {
            let _ = std::fs::remove_file(&temp);
            Err(error)
        }
    }
}
