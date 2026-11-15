//! The computer's sound and microphones on Windows, through WASAPI: the default output device's loopback for the
//! computer's sound, and the microphone's own stream. Windows converts both to 48 kHz float on the way, and Media
//! Foundation writes them as AAC in an `.m4a`, timed on the same clock as the screen's frames.
//!
//! The loopback sends nothing while nothing plays, so the gaps are filled with silence from the packets' times.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::thread::JoinHandle;
use std::time::Duration;

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::E_ACCESSDENIED;
use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, DEVICE_STATE_ACTIVE,
    IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX,
    eCapture, eConsole, eRender,
};
use windows::Win32::Media::MediaFoundation::{
    IMFSinkWriter, MF_MT_AUDIO_AVG_BYTES_PER_SECOND, MF_MT_AUDIO_BITS_PER_SAMPLE,
    MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_MAJOR_TYPE,
    MF_MT_SUBTYPE, MF_SINK_WRITER_DISABLE_THROTTLING, MF_TRANSCODE_CONTAINERTYPE, MFAudioFormat_AAC,
    MFAudioFormat_PCM, MFCreateAttributes, MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample,
    MFCreateSinkWriterFromURL, MFMediaType_Audio, MFTranscodeContainerType_MPEG4,
};
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, STGM_READ};
use windows::core::{HSTRING, IUnknown, PWSTR};

use super::record::start_systems;
use crate::Device;

/// Samples per second, as on the Mac.
const RATE: u32 = 48_000;
/// A gap longer than this between packets is filled with silence; shorter ones are the device's jitter.
const GAP: f64 = 0.02;
/// IEEE float samples, in a `WAVEFORMATEX`.
const FLOAT: u16 = 3;

/// What to record.
pub enum Source {
    /// What the computer plays, from the default output device.
    System,
    /// A microphone by its endpoint id, or the default one.
    Microphone(Option<String>),
}

/// When a recorded sound starts and ends, in seconds on the frames' clock; `None` when nothing was heard.
type Heard = Result<Option<(f64, f64)>, String>;

/// A sound being recorded on its own thread.
pub struct AudioRecorder {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Heard>>,
}

impl AudioRecorder {
    /// Starts recording `source` into `path` and returns once the device is running, or says why it can't.
    pub fn start(source: Source, path: &Path) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready, started) = channel();
        let thread = {
            let (stop, path) = (stop.clone(), path.to_path_buf());
            std::thread::Builder::new()
                .name("sound".into())
                .spawn(move || run(&source, &path, &stop, &ready))
                .map_err(|e| e.to_string())?
        };
        match started.recv() {
            Ok(Ok(())) => Ok(Self { stop, thread: Some(thread) }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => Err(match thread.join() {
                Ok(Err(error)) => error,
                _ => "the sound recorder stopped unexpectedly".into(),
            }),
        }
    }

    /// Stops and finishes the file. Says when the sound starts and ends on the frames' clock, or `None` when nothing
    /// was heard (the file is then removed).
    pub fn stop(mut self) -> Heard {
        self.stop.store(true, Ordering::Relaxed);
        match self.thread.take().map(JoinHandle::join) {
            Some(Ok(result)) => result,
            _ => Err("the sound recorder stopped unexpectedly".into()),
        }
    }
}

impl Drop for AudioRecorder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn message(error: &windows::core::Error) -> String {
    error.message()
}

/// The device, its stream and the file, started.
fn prepare(source: &Source, path: &Path) -> Result<(IAudioClient, IAudioCaptureClient, AacWriter), String> {
    start_systems().map_err(|e| e.to_string())?;
    let microphone = matches!(source, Source::Microphone(_));
    let missing = |_: windows::core::Error| {
        if microphone {
            "there's no microphone".to_string()
        } else {
            "there's no sound output to record".into()
        }
    };
    // SAFETY: COM calls on this thread with valid arguments; the format outlives the call that reads it.
    unsafe {
        let devices: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None::<&IUnknown>, CLSCTX_ALL).map_err(|e| message(&e))?;
        let device = match source {
            Source::System => devices.GetDefaultAudioEndpoint(eRender, eConsole).map_err(missing)?,
            Source::Microphone(None) => {
                devices.GetDefaultAudioEndpoint(eCapture, eConsole).map_err(missing)?
            }
            Source::Microphone(Some(id)) => devices
                .GetDevice(&HSTRING::from(id.as_str()))
                .map_err(|_| "that microphone isn't connected".to_string())?,
        };
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None).map_err(|e| message(&e))?;
        let channels: u16 = if microphone { 1 } else { 2 };
        let format = WAVEFORMATEX {
            wFormatTag: FLOAT,
            nChannels: channels,
            nSamplesPerSec: RATE,
            nAvgBytesPerSec: RATE * 4 * u32::from(channels),
            nBlockAlign: 4 * channels,
            wBitsPerSample: 32,
            cbSize: 0,
        };
        // Windows converts the device's own format to this one.
        let mut flags = AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
        if !microphone {
            flags |= AUDCLNT_STREAMFLAGS_LOOPBACK;
        }
        // A second of buffer: far more than the 10 ms between reads.
        client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 10_000_000, 0, &format, None).map_err(|e| {
            if e.code() == E_ACCESSDENIED && microphone {
                "the microphone isn't allowed: turn on microphone access for desktop apps in Settings › Privacy & \
                 security › Microphone"
                    .to_string()
            } else {
                message(&e)
            }
        })?;
        let capture: IAudioCaptureClient = client.GetService().map_err(|e| message(&e))?;
        let writer = AacWriter::new(path, channels)?;
        client.Start().map_err(|e| message(&e))?;
        Ok((client, capture, writer))
    }
}

/// Records until asked to stop, reading what has arrived every 10 ms.
fn run(source: &Source, path: &PathBuf, stop: &AtomicBool, ready: &Sender<Result<(), String>>) -> Heard {
    let (client, capture, mut writer) = match prepare(source, path) {
        Ok(parts) => {
            let _ = ready.send(Ok(()));
            parts
        }
        Err(error) => {
            let _ = ready.send(Err(error.clone()));
            let _ = std::fs::remove_file(path);
            return Err(error);
        }
    };
    let channels = usize::from(writer.channels);
    let rate = f64::from(RATE);
    // When the first sound arrived, and how many frames are written since.
    let (mut start, mut frames) = (None::<f64>, 0u64);
    let mut pcm: Vec<f32> = Vec::new();
    // A second of silence, written as many times as a gap needs.
    let quiet = vec![0f32; RATE as usize * channels];
    let result = (|| -> Result<(), String> {
        loop {
            let stopping = stop.load(Ordering::Relaxed);
            // SAFETY: each packet's data is read before it is released, within the frame count it gives.
            unsafe {
                while capture.GetNextPacketSize().map_err(|e| message(&e))? > 0 {
                    let (mut data, mut count, mut flags, mut qpc) = (std::ptr::null_mut(), 0u32, 0u32, 0u64);
                    capture
                        .GetBuffer(&mut data, &mut count, &mut flags, None, Some(&raw mut qpc))
                        .map_err(|e| message(&e))?;
                    // The packet's time, on the frames' clock: the performance counter in 100 ns units.
                    let at = qpc as f64 / 1e7;
                    let first = *start.get_or_insert(at);
                    let behind = at - (first + frames as f64 / rate);
                    if behind > GAP {
                        let mut gap = (behind * rate).round() as usize;
                        while gap > 0 {
                            let n = gap.min(RATE as usize);
                            writer.write(&quiet[..n * channels])?;
                            frames += n as u64;
                            gap -= n;
                        }
                    }
                    pcm.clear();
                    let samples = count as usize * channels;
                    if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                        pcm.resize(pcm.len() + samples, 0.0);
                    } else {
                        pcm.extend_from_slice(std::slice::from_raw_parts(data.cast::<f32>(), samples));
                    }
                    capture.ReleaseBuffer(count).map_err(|e| message(&e))?;
                    writer.write(&pcm)?;
                    frames += (pcm.len() / channels) as u64;
                }
            }
            if stopping {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    // SAFETY: stops the stream this thread started.
    let _ = unsafe { client.Stop() };
    result?;
    match start {
        Some(first) if frames > 0 => {
            writer.finish()?;
            Ok(Some((first, first + frames as f64 / rate)))
        }
        _ => {
            drop(writer);
            let _ = std::fs::remove_file(path);
            Ok(None)
        }
    }
}

/// AAC in an `.m4a`, from float samples.
struct AacWriter {
    writer: IMFSinkWriter,
    stream: u32,
    channels: u16,
    /// Frames written so far, which times the next sample.
    written: u64,
}

impl AacWriter {
    fn new(path: &Path, channels: u16) -> Result<Self, String> {
        let error = |e: windows::core::Error| format!("couldn't start the sound file: {}", e.message());
        let block = u32::from(channels) * 2;
        // SAFETY: Media Foundation calls with valid objects; the path outlives the call that reads it.
        unsafe {
            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 2).map_err(error)?;
            let attributes = attributes.ok_or("Media Foundation is missing")?;
            attributes
                .SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4)
                .map_err(error)?;
            attributes.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1).map_err(error)?;
            let writer = MFCreateSinkWriterFromURL(&HSTRING::from(path), None, &attributes).map_err(error)?;

            let output = MFCreateMediaType().map_err(error)?;
            output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(error)?;
            output.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC).map_err(error)?;
            output.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).map_err(error)?;
            output.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, RATE).map_err(error)?;
            output.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, u32::from(channels)).map_err(error)?;
            // 192 kbps in stereo and 128 kbps in mono: two of the rates Windows' AAC encoder offers.
            let bytes = if channels > 1 { 24_000 } else { 16_000 };
            output.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, bytes).map_err(error)?;
            let stream = writer.AddStream(&output).map_err(error)?;

            let input = MFCreateMediaType().map_err(error)?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(error)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM).map_err(error)?;
            input.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).map_err(error)?;
            input.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, RATE).map_err(error)?;
            input.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, u32::from(channels)).map_err(error)?;
            input.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, block).map_err(error)?;
            input.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, RATE * block).map_err(error)?;
            writer.SetInputMediaType(stream, &input, None).map_err(error)?;
            writer.BeginWriting().map_err(error)?;
            Ok(Self { writer, stream, channels, written: 0 })
        }
    }

    /// Adds interleaved samples, as 16-bit PCM.
    fn write(&mut self, pcm: &[f32]) -> Result<(), String> {
        let frames = (pcm.len() / usize::from(self.channels)) as u64;
        if frames == 0 {
            return Ok(());
        }
        let length = (pcm.len() * 2) as u32;
        let time = |frame: u64| (i128::from(frame) * 10_000_000 / i128::from(RATE)) as i64;
        let (at, end) = (time(self.written), time(self.written + frames));
        let error = |e: windows::core::Error| format!("sound couldn't be saved: {}", e.message());
        // SAFETY: the buffer is locked while it is filled, with exactly its length.
        unsafe {
            let buffer = MFCreateMemoryBuffer(length).map_err(error)?;
            let mut data = std::ptr::null_mut();
            buffer.Lock(&mut data, None, None).map_err(error)?;
            let out = std::slice::from_raw_parts_mut(data.cast::<i16>(), pcm.len());
            for (o, &s) in out.iter_mut().zip(pcm) {
                *o = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            }
            buffer.Unlock().map_err(error)?;
            buffer.SetCurrentLength(length).map_err(error)?;
            let sample = MFCreateSample().map_err(error)?;
            sample.AddBuffer(&buffer).map_err(error)?;
            sample.SetSampleTime(at).map_err(error)?;
            sample.SetSampleDuration(end - at).map_err(error)?;
            self.writer.WriteSample(self.stream, &sample).map_err(error)?;
        }
        self.written += frames;
        Ok(())
    }

    fn finish(self) -> Result<(), String> {
        // SAFETY: finishes the file; the writer isn't used again.
        unsafe { self.writer.Finalize() }
            .map_err(|e| format!("the sound file couldn't be finished: {}", e.message()))
    }
}

/// The text a COM call handed back, freeing it.
///
/// # Safety
/// `text` must be a string allocated with `CoTaskMemAlloc`, or null.
unsafe fn take_text(text: PWSTR) -> Option<String> {
    if text.is_null() {
        return None;
    }
    // SAFETY: the caller promises a valid, owned string, which is freed once copied.
    unsafe {
        let copy = text.to_string().ok();
        CoTaskMemFree(Some(text.0 as *const _));
        copy
    }
}

fn device_id(device: &IMMDevice) -> Option<String> {
    // SAFETY: the id is allocated for the caller to free.
    unsafe { take_text(device.GetId().ok()?) }
}

fn device_name(device: &IMMDevice) -> Option<String> {
    // SAFETY: reads one property and clears the value it hands back.
    unsafe {
        let store = device.OpenPropertyStore(STGM_READ).ok()?;
        let mut value = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
        let name = PropVariantToStringAlloc(&value).ok().and_then(|text| take_text(text));
        let _ = PropVariantClear(&mut value);
        name
    }
}

/// Connected microphones, the default first.
pub fn microphones() -> Vec<Device> {
    if start_systems().is_err() {
        return Vec::new();
    }
    // SAFETY: COM calls with valid arguments; every object is owned.
    let listed = unsafe {
        (|| -> windows::core::Result<Vec<Device>> {
            let devices: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None::<&IUnknown>, CLSCTX_ALL)?;
            let default =
                devices.GetDefaultAudioEndpoint(eCapture, eConsole).ok().and_then(|d| device_id(&d));
            let list = devices.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
            let mut found = Vec::new();
            for i in 0..list.GetCount()? {
                let device = list.Item(i)?;
                let Some(id) = device_id(&device) else { continue };
                let name = device_name(&device).unwrap_or_else(|| "Microphone".into());
                found.push(Device { is_default: default.as_deref() == Some(id.as_str()), id, name });
            }
            Ok(found)
        })()
    };
    let mut found = listed.unwrap_or_default();
    found.sort_by_key(|d| !d.is_default);
    found
}
