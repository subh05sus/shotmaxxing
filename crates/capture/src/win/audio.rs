//! The computer's sound and microphones on Windows, through WASAPI: a loopback of everything that plays except this
//! app (process loopback, on Windows 11 and Server 2022 or later; elsewhere the default output device's loopback,
//! which hears this app too), and the microphone's own stream. Windows converts both to 48 kHz float on the way, and
//! Media Foundation writes them as AAC in an `.m4a`, timed on the same clock as the screen's frames.
//!
//! A loopback sends nothing while nothing plays, so the gaps are filled with silence from the packets' times.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, SyncSender, channel, sync_channel};
use std::thread::JoinHandle;
use std::time::Duration;

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{CloseHandle, E_ACCESSDENIED, HANDLE};
use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, DEVICE_STATE_ACTIVE,
    IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, WAVEFORMATEX,
    eCapture, eConsole, eRender,
};
use windows::Win32::Media::Audio::{
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0,
    AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS,
    ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
    IActivateAudioInterfaceCompletionHandler, IActivateAudioInterfaceCompletionHandler_Impl,
    PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE, VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
};
use windows::Win32::Media::MediaFoundation::{
    IMFSinkWriter, MF_MT_AUDIO_AVG_BYTES_PER_SECOND, MF_MT_AUDIO_BITS_PER_SAMPLE,
    MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_MAJOR_TYPE,
    MF_MT_SUBTYPE, MF_SINK_WRITER_DISABLE_THROTTLING, MF_TRANSCODE_CONTAINERTYPE, MFAudioFormat_AAC,
    MFAudioFormat_PCM, MFCreateAttributes, MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample,
    MFCreateSinkWriterFromURL, MFMediaType_Audio, MFTranscodeContainerType_MPEG4,
};
use windows::Win32::System::Com::StructuredStorage::{
    PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0, PropVariantClear,
    PropVariantToStringAlloc,
};
use windows::Win32::System::Com::{BLOB, CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, STGM_READ};
use windows::Win32::System::Threading::CreateEventW;
use windows::Win32::System::Variant::VT_BLOB;
use windows::core::{HRESULT, HSTRING, IUnknown, Interface, PWSTR, Ref, implement};

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

/// A microphone recorded on its own, for a voice-over.
pub struct Voice {
    recorder: AudioRecorder,
}

impl Voice {
    /// Starts recording `device` (the default microphone when `None`) into `output`, and returns once it runs.
    pub fn start(device: Option<String>, output: &Path) -> Result<Self, crate::CaptureError> {
        AudioRecorder::start(Source::Microphone(device), output)
            .map(|recorder| Self { recorder })
            .map_err(crate::CaptureError::Device)
    }

    /// Stops and finishes the file. Returns how long it plays, in seconds; fails (and leaves no file) when the
    /// microphone sent nothing.
    pub fn stop(self) -> Result<f64, crate::CaptureError> {
        match self.recorder.stop() {
            Ok(Some((first, end))) if end > first => Ok(end - first),
            Ok(_) => Err(crate::CaptureError::Device("The microphone didn't send any sound.".into())),
            Err(error) => Err(crate::CaptureError::Device(error)),
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

/// Tells the waiting thread that Windows has finished making the process loopback client.
#[implement(IActivateAudioInterfaceCompletionHandler)]
struct Activated(SyncSender<()>);

impl IActivateAudioInterfaceCompletionHandler_Impl for Activated_Impl {
    fn ActivateCompleted(
        &self,
        _operation: Ref<IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        let _ = self.0.try_send(());
        Ok(())
    }
}

/// A loopback client for everything that plays except this app and the processes it started, or why Windows
/// can't make one (it's new in Windows 11 and Server 2022).
fn process_loopback() -> Result<IAudioClient, String> {
    // On the heap, and kept if Windows doesn't answer in time, as it may still read them afterwards.
    let mut params = Box::new(AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: std::process::id(),
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE,
            },
        },
    });
    let blob = Box::new(PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_BLOB,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 {
                    blob: BLOB {
                        cbSize: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                        pBlobData: (&raw mut *params).cast(),
                    },
                },
            }),
        },
    });
    let (done, finished) = sync_channel(1);
    let handler: IActivateAudioInterfaceCompletionHandler = Activated(done).into();
    // SAFETY: the parameters and the PROPVARIANT pointing at them stay alive until Windows is done with them.
    unsafe {
        let operation = ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some(&raw const *blob),
            &handler,
        )
        .map_err(|e| message(&e))?;
        if finished.recv_timeout(Duration::from_secs(5)).is_err() {
            std::mem::forget((params, blob));
            return Err("Windows didn't finish setting up the sound recording".into());
        }
        let (mut result, mut activated) = (HRESULT(0), None);
        operation.GetActivateResult(&mut result, &mut activated).map_err(|e| message(&e))?;
        result.ok().map_err(|e| message(&e))?;
        activated.ok_or("Windows made no sound client")?.cast::<IAudioClient>().map_err(|e| message(&e))
    }
}

/// A started stream: its client, what reads it, the file, and the event an event-driven client signals (closed when
/// the stream stops).
type Prepared = (IAudioClient, IAudioCaptureClient, AacWriter, Option<HANDLE>);

/// The device, its stream and the file, started.
fn prepare(source: &Source, path: &Path) -> Result<Prepared, String> {
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
        // The computer's sound leaves this app's own out where Windows can (process loopback, event-driven as
        // Windows' own sample and Chromium run it); elsewhere the whole output is recorded.
        let leaving_out = if microphone {
            None
        } else {
            match process_loopback().and_then(|client| {
                let event = CreateEventW(None, false, false, None).map_err(|e| message(&e))?;
                // The flags of Windows' own process loopback sample.
                let process_flags = AUDCLNT_STREAMFLAGS_LOOPBACK
                    | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM;
                let started = client
                    .Initialize(AUDCLNT_SHAREMODE_SHARED, process_flags, 10_000_000, 0, &format, None)
                    .and_then(|()| client.SetEventHandle(event));
                match started {
                    Ok(()) => Ok((client, event)),
                    Err(error) => {
                        let _ = CloseHandle(event);
                        Err(message(&error))
                    }
                }
            }) {
                Ok(started) => Some(started),
                Err(why) => {
                    tracing::info!(%why, "recording all of the computer's sound, this app's too");
                    None
                }
            }
        };
        let (client, event) = match leaving_out {
            Some((client, event)) => (client, Some(event)),
            None => {
                let client: IAudioClient = device.Activate(CLSCTX_ALL, None).map_err(|e| message(&e))?;
                // A second of buffer: far more than the 10 ms between reads.
                client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 10_000_000, 0, &format, None).map_err(|e| {
                    if e.code() == E_ACCESSDENIED && microphone {
                        "the microphone isn't allowed: turn on microphone access for desktop apps in Settings › \
                         Privacy & security › Microphone"
                            .to_string()
                    } else {
                        message(&e)
                    }
                })?;
                (client, None)
            }
        };
        let close = |event: Option<HANDLE>| {
            if let Some(event) = event {
                let _ = CloseHandle(event);
            }
        };
        let capture: IAudioCaptureClient = match client.GetService() {
            Ok(capture) => capture,
            Err(error) => {
                close(event);
                return Err(message(&error));
            }
        };
        let writer = match AacWriter::new(path, channels) {
            Ok(writer) => writer,
            Err(error) => {
                close(event);
                return Err(error);
            }
        };
        if let Err(error) = client.Start() {
            close(event);
            return Err(message(&error));
        }
        Ok((client, capture, writer, event))
    }
}

/// Records until asked to stop, reading what has arrived every 10 ms.
fn run(source: &Source, path: &PathBuf, stop: &AtomicBool, ready: &Sender<Result<(), String>>) -> Heard {
    let (client, capture, mut writer, event) = match prepare(source, path) {
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
                    // The packet's time, on the frames' clock: the performance counter in 100 ns units. A time no
                    // one-second buffer could hold (or none, which a process loopback may give) is replaced by
                    // now, less the packet.
                    let (reported, now) = (qpc as f64 / 1e7, super::clock::now());
                    let at = if qpc != 0 && (reported - now).abs() < 1.5 {
                        reported
                    } else {
                        now - f64::from(count) / rate
                    };
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
    // SAFETY: stops the stream this thread started, then closes the event it signalled.
    let _ = unsafe { client.Stop() };
    if let Some(event) = event {
        // SAFETY: the event was made for this stream, which no longer signals it.
        let _ = unsafe { CloseHandle(event) };
    }
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
