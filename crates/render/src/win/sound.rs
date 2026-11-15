//! Reads a sound file for the mixer with Media Foundation's source reader, as 48 kHz float stereo. Windows converts
//! the rate and channels on the way where it can; otherwise this does, so any file Windows can decode is heard.

use std::path::Path;

use windows::Win32::Media::MediaFoundation::{
    IMFSourceReader, MF_MT_AUDIO_AVG_BYTES_PER_SECOND, MF_MT_AUDIO_BITS_PER_SAMPLE,
    MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_MAJOR_TYPE,
    MF_MT_SUBTYPE, MF_PD_DURATION, MF_SOURCE_READER_ALL_STREAMS, MF_SOURCE_READER_FIRST_AUDIO_STREAM,
    MF_SOURCE_READER_MEDIASOURCE, MF_SOURCE_READERF_ENDOFSTREAM, MFAudioFormat_Float, MFCreateMediaType,
    MFCreateSourceReaderFromURL, MFMediaType_Audio,
};
use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PropVariantClear};
use windows::Win32::System::Variant::VT_I8;
use windows::core::{GUID, HSTRING};

use super::{start, system};
use crate::RenderError;
use crate::mix::{Opened, RATE, Source};

const AUDIO: u32 = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;

pub struct AudioFile {
    reader: IMFSourceReader,
    seconds: f64,
    /// What the reader hands back, when Windows couldn't convert it to 48 kHz stereo itself.
    rate: u32,
    channels: usize,
    /// Decoded frames, as 48 kHz stereo, and how far into them reading is.
    pending: Vec<f32>,
    used: usize,
    /// After a seek, the frame reading should start at: earlier frames the reader hands back are dropped.
    seek_to: Option<u64>,
    /// Where a rate conversion is between source frames, and the last frame of the previous buffer.
    phase: f64,
    previous: [f32; 2],
    done: bool,
}

impl AudioFile {
    /// Opens `path` for reading, or says it holds no sound Windows can decode.
    pub fn open(path: &Path) -> Opened {
        start()?;
        // SAFETY: Media Foundation calls with valid objects; the path outlives the call that reads it, and the
        // duration's PROPVARIANT is cleared after it is read.
        unsafe {
            let Ok(reader) = MFCreateSourceReaderFromURL(&HSTRING::from(path), None) else { return Ok(None) };
            reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false).map_err(system)?;
            if reader.SetStreamSelection(AUDIO, true).is_err() {
                return Ok(None);
            }
            let float = MFCreateMediaType().map_err(system)?;
            float.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(system)?;
            float.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_Float).map_err(system)?;
            float.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 32).map_err(system)?;
            let exact = float.clone();
            exact.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, RATE).map_err(system)?;
            exact.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2).map_err(system)?;
            exact.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 8).map_err(system)?;
            exact.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, RATE * 8).map_err(system)?;
            if reader.SetCurrentMediaType(AUDIO, None, &exact).is_err()
                && reader.SetCurrentMediaType(AUDIO, None, &float).is_err()
            {
                return Ok(None);
            }
            let format = reader.GetCurrentMediaType(AUDIO).map_err(system)?;
            let rate = format.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND).map_err(system)?;
            let channels = format.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS).map_err(system)? as usize;
            if rate == 0 || channels == 0 {
                return Ok(None);
            }
            let mut duration = reader
                .GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION)
                .map_err(system)?;
            let ticks = duration.Anonymous.Anonymous.Anonymous.uhVal;
            let _ = PropVariantClear(&mut duration);
            Ok(Some(Box::new(Self {
                reader,
                seconds: ticks as f64 / 1e7,
                rate,
                channels,
                pending: Vec::new(),
                used: 0,
                seek_to: None,
                phase: 0.0,
                previous: [0.0; 2],
                done: false,
            })))
        }
    }

    /// Decodes the next buffer into `pending`. `false` at the end.
    fn decode(&mut self) -> Result<bool, RenderError> {
        while !self.done {
            let (mut flags, mut timestamp, mut sample) = (0u32, 0i64, None);
            // SAFETY: the out-pointers are valid for the call; the buffer is locked while it is read, within the
            // length it gives.
            unsafe {
                self.reader
                    .ReadSample(
                        AUDIO,
                        0,
                        None,
                        Some(&raw mut flags),
                        Some(&raw mut timestamp),
                        Some(&raw mut sample),
                    )
                    .map_err(|e| {
                        RenderError::System(format!("a sound couldn't be decoded: {}", e.message()))
                    })?;
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    self.done = true;
                }
                let Some(sample) = sample else { continue };
                let buffer = sample.ConvertToContiguousBuffer().map_err(system)?;
                let (mut data, mut length) = (std::ptr::null_mut(), 0u32);
                buffer.Lock(&mut data, None, Some(&raw mut length)).map_err(system)?;
                let floats = std::slice::from_raw_parts(data.cast::<f32>(), length as usize / 4);
                self.pending.clear();
                self.used = 0;
                self.convert(floats);
                buffer.Unlock().map_err(system)?;
                if let Some(target) = self.seek_to.take() {
                    // Drop what comes before the frame asked for.
                    let first = (timestamp.max(0) as f64 * f64::from(RATE) / 1e7).round() as u64;
                    let early = (target.saturating_sub(first) as usize * 2).min(self.pending.len());
                    self.used = early;
                }
                if self.used < self.pending.len() {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// Adds decoded samples to `pending` as 48 kHz stereo.
    fn convert(&mut self, floats: &[f32]) {
        let channels = self.channels;
        let frame = |i: usize| -> [f32; 2] {
            let at = i * channels;
            if channels == 1 { [floats[at]; 2] } else { [floats[at], floats[at + 1]] }
        };
        let count = floats.len() / channels;
        if self.rate == RATE {
            for i in 0..count {
                self.pending.extend_from_slice(&frame(i));
            }
        } else {
            // A straight line between source frames, carried across buffers.
            let step = f64::from(self.rate) / f64::from(RATE);
            while self.phase < count as f64 {
                let i = self.phase.floor();
                let t = (self.phase - i) as f32;
                let a = if i < 1.0 { self.previous } else { frame(i as usize - 1) };
                let b = frame(i as usize);
                self.pending.push(a[0] + (b[0] - a[0]) * t);
                self.pending.push(a[1] + (b[1] - a[1]) * t);
                self.phase += step;
            }
            self.phase -= count as f64;
            if count > 0 {
                self.previous = frame(count - 1);
            }
        }
    }
}

impl Source for AudioFile {
    fn read(&mut self, out: &mut [f32]) -> Result<usize, RenderError> {
        let want = out.len() / 2;
        let mut done = 0;
        while done < want {
            if self.used >= self.pending.len() {
                if !self.decode()? {
                    break;
                }
                continue;
            }
            let take = ((self.pending.len() - self.used) / 2).min(want - done);
            out[done * 2..(done + take) * 2].copy_from_slice(&self.pending[self.used..self.used + take * 2]);
            self.used += take * 2;
            done += take;
        }
        Ok(done)
    }

    fn seek(&mut self, frame: u64) -> Result<(), RenderError> {
        let mut position = PROPVARIANT::default();
        // SAFETY: a VT_I8 PROPVARIANT holds its value in `hVal` and owns nothing.
        unsafe {
            let value = &mut *position.Anonymous.Anonymous;
            value.vt = VT_I8;
            value.Anonymous.hVal = (frame as f64 * 1e7 / f64::from(RATE)).round() as i64;
            self.reader.SetCurrentPosition(&GUID::zeroed(), &position).map_err(system)?;
        }
        self.pending.clear();
        self.used = 0;
        self.seek_to = Some(frame);
        self.phase = 0.0;
        self.done = false;
        Ok(())
    }

    fn seconds(&self) -> f64 {
        self.seconds
    }
}
