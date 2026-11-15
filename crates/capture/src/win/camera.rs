//! The camera on Windows. Media Foundation's source reader takes the camera's frames, in its format closest to
//! 1280 × 720 at 30 a second as on the Mac, and hands them over as NV12; the sink writer encodes them to H.264 in a
//! fragmented MP4, so the file plays however the recording ends, on the graphics card where it can. The frames are
//! placed on the clock the screen's frames use.

use std::ffi::c_void;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::thread::JoinHandle;

use windows::Win32::Foundation::E_ACCESSDENIED;
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, IMFMediaSource, IMFMediaType, IMFSinkWriter, IMFSourceReader,
    MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
    MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE,
    MF_MT_MPEG2_PROFILE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS,
    MF_SINK_WRITER_DISABLE_THROTTLING, MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING,
    MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READERF_ERROR,
    MF_TRANSCODE_CONTAINERTYPE, MFCreateAttributes, MFCreateMediaType, MFCreateSinkWriterFromURL,
    MFCreateSourceReaderFromMediaSource, MFEnumDeviceSources, MFMediaType_Video,
    MFTranscodeContainerType_FMPEG4, MFVideoFormat_H264, MFVideoFormat_MJPG, MFVideoFormat_NV12,
    MFVideoInterlace_Progressive, eAVEncH264VProfile_High,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::core::{GUID, HSTRING, PWSTR};

use super::clock;
use super::record::start_systems;
use crate::Device;

const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

/// When the first frame was taken and when the last one ends, on the frames' clock; `None` when no frame came.
type Filmed = Result<Option<(f64, f64)>, String>;

fn message(error: windows::core::Error) -> String {
    error.message()
}

/// The cameras Media Foundation can film with, as objects that start them.
fn devices() -> Result<Vec<IMFActivate>, String> {
    // SAFETY: Media Foundation fills an array it allocates with `count` objects. Each is taken out of the array
    // before the array is freed.
    unsafe {
        let mut attributes = None;
        MFCreateAttributes(&mut attributes, 1).map_err(message)?;
        let attributes = attributes.ok_or("Media Foundation is missing")?;
        attributes
            .SetGUID(&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID)
            .map_err(message)?;
        let (mut list, mut count) = (std::ptr::null_mut(), 0u32);
        MFEnumDeviceSources(&attributes, &mut list, &mut count).map_err(message)?;
        if list.is_null() {
            return Ok(Vec::new());
        }
        let found = std::slice::from_raw_parts_mut(list, count as usize)
            .iter_mut()
            .filter_map(Option::take)
            .collect();
        CoTaskMemFree(Some(list as *const c_void));
        Ok(found)
    }
}

/// A text attribute of a camera.
fn text(device: &IMFActivate, key: &GUID) -> Option<String> {
    let (mut value, mut length) = (PWSTR::null(), 0u32);
    // SAFETY: the string is allocated for the caller, read once and freed.
    unsafe {
        device.GetAllocatedString(key, &mut value, &mut length).ok()?;
        let text = value.to_string().ok();
        CoTaskMemFree(Some(value.0 as *const c_void));
        text
    }
}

fn id(device: &IMFActivate) -> Option<String> {
    text(device, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK)
}

/// The cameras, as Windows lists them. Windows has no default camera, so the first stands in for one.
pub fn cameras() -> Vec<Device> {
    if start_systems().is_err() {
        return Vec::new();
    }
    let devices = devices().unwrap_or_default();
    devices
        .iter()
        .filter_map(|device| {
            let name = text(device, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME).unwrap_or_else(|| "Camera".into());
            Some(Device { id: id(device)?, name, is_default: false })
        })
        .enumerate()
        .map(|(i, device)| Device { is_default: i == 0, ..device })
        .collect()
}

/// A camera being filmed into a file on a thread of its own.
pub struct CameraRecorder {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Filmed>>,
}

impl CameraRecorder {
    /// Starts filming the camera with this id (or the first one) into `path`, and returns once it's running, or
    /// says why it can't.
    pub fn start(id: Option<&str>, path: &Path) -> Result<Self, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready, started) = channel();
        let thread = {
            let (stop, path, id) = (stop.clone(), path.to_path_buf(), id.map(str::to_owned));
            std::thread::Builder::new()
                .name("camera".into())
                .spawn(move || run(id.as_deref(), &path, &stop, &ready))
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
                _ => "the camera stopped unexpectedly".into(),
            }),
        }
    }

    /// Stops and finishes the file. Says when the camera's video starts and ends on the frames' clock, or `None`
    /// when no frame came (the file is then removed).
    pub fn stop(mut self) -> Filmed {
        self.stop.store(true, Ordering::Relaxed);
        match self.thread.take().map(JoinHandle::join) {
            Some(Ok(result)) => result,
            _ => Err("the camera stopped unexpectedly".into()),
        }
    }
}

impl Drop for CameraRecorder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(id: Option<&str>, path: &Path, stop: &AtomicBool, ready: &Sender<Result<(), String>>) -> Filmed {
    let (source, reader, writer, stream) = match prepare(id, path) {
        Ok(parts) => {
            let _ = ready.send(Ok(()));
            parts
        }
        Err(error) => {
            let _ = ready.send(Err(error.clone()));
            return Err(error);
        }
    };
    let filmed = film(&reader, &writer, stream, stop);
    // SAFETY: finishes the file and lets the camera go; neither is used again.
    let finished = unsafe { writer.Finalize() };
    // SAFETY: as above.
    let _ = unsafe { source.Shutdown() };
    match filmed? {
        Some(range) => {
            finished.map_err(|e| format!("the camera's video couldn't be finished: {}", e.message()))?;
            Ok(Some(range))
        }
        None => {
            let _ = std::fs::remove_file(path);
            Ok(None)
        }
    }
}

/// The camera started, its reader and the file's writer ready.
fn prepare(
    id: Option<&str>,
    path: &Path,
) -> Result<(IMFMediaSource, IMFSourceReader, IMFSinkWriter, u32), String> {
    start_systems().map_err(|e| e.to_string())?;
    let devices = devices()?;
    let device = match id {
        Some(wanted) => devices.iter().find(|d| self::id(d).as_deref() == Some(wanted)),
        None => devices.first(),
    }
    .ok_or_else(|| {
        if devices.is_empty() { "there's no camera" } else { "that camera isn't connected" }.to_string()
    })?;
    // SAFETY: starts the camera; it's shut down again if anything after this fails.
    let source: IMFMediaSource = unsafe { device.ActivateObject() }.map_err(|e| {
        if e.code() == E_ACCESSDENIED {
            "the camera isn't allowed: turn on camera access for desktop apps in Settings › Privacy & security › \
             Camera"
                .to_string()
        } else {
            format!("the camera didn't start, so another app may be using it ({})", e.message())
        }
    })?;
    match open(&source, path) {
        Ok((reader, writer, stream)) => Ok((source, reader, writer, stream)),
        Err(error) => {
            // SAFETY: lets the camera go; it isn't used again.
            let _ = unsafe { source.Shutdown() };
            Err(error)
        }
    }
}

fn pack(high: u32, low: u32) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}

fn unpack(value: u64) -> (u32, u32) {
    ((value >> 32) as u32, value as u32)
}

fn frames_per_second(rate: u64) -> f64 {
    let (frames, seconds) = unpack(rate);
    f64::from(frames) / f64::from(seconds.max(1))
}

/// The camera's own format closest to 1280 × 720 at 30 frames a second, as the Mac films: a smooth rate first, then
/// the size (larger rather than smaller when there's no 720), then the rate nearest 30, then frames it doesn't
/// have to compress.
fn best_format(reader: &IMFSourceReader) -> Result<IMFMediaType, String> {
    let mut best: Option<((bool, i64, i64, bool), IMFMediaType)> = None;
    for i in 0u32.. {
        // SAFETY: asks for one of the camera's formats by number; past the last one it fails.
        let Ok(format) = (unsafe { reader.GetNativeMediaType(VIDEO, i) }) else { break };
        // SAFETY: reads attributes of a media type.
        let (size, rate, subtype) = unsafe {
            (
                format.GetUINT64(&MF_MT_FRAME_SIZE),
                format.GetUINT64(&MF_MT_FRAME_RATE),
                format.GetGUID(&MF_MT_SUBTYPE),
            )
        };
        let (Ok(size), Ok(rate)) = (size, rate) else { continue };
        let (width, height) = unpack(size);
        if width == 0 || height == 0 {
            continue;
        }
        let fps = frames_per_second(rate);
        let lines = i64::from(height) - 720;
        let size = if lines >= 0 { -lines } else { 2 * lines };
        let score = (
            fps >= 24.0,
            size,
            -((fps - 30.0).abs() * 100.0) as i64,
            subtype.ok() != Some(MFVideoFormat_MJPG),
        );
        if best.as_ref().is_none_or(|(b, _)| score > *b) {
            best = Some((score, format));
        }
    }
    best.map(|(_, format)| format).ok_or_else(|| "the camera offers no pictures".into())
}

/// A reader that hands over the camera's frames as NV12, and a writer for them.
fn open(source: &IMFMediaSource, path: &Path) -> Result<(IMFSourceReader, IMFSinkWriter, u32), String> {
    // SAFETY: Media Foundation calls with valid objects; the path outlives the call that reads it.
    unsafe {
        let mut attributes = None;
        MFCreateAttributes(&mut attributes, 1).map_err(message)?;
        let attributes = attributes.ok_or("Media Foundation is missing")?;
        // The reader decodes MJPEG and converts colours itself.
        attributes.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1).map_err(message)?;
        let reader = MFCreateSourceReaderFromMediaSource(source, &attributes).map_err(message)?;
        let native = best_format(&reader)?;
        reader.SetCurrentMediaType(VIDEO, None, &native).map_err(message)?;
        let nv12 = MFCreateMediaType().map_err(message)?;
        nv12.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(message)?;
        nv12.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12).map_err(message)?;
        reader
            .SetCurrentMediaType(VIDEO, None, &nv12)
            .map_err(|e| format!("the camera's pictures can't be converted ({})", e.message()))?;
        let format = reader.GetCurrentMediaType(VIDEO).map_err(message)?;
        let (width, height) = unpack(format.GetUINT64(&MF_MT_FRAME_SIZE).map_err(message)?);
        let rate = format.GetUINT64(&MF_MT_FRAME_RATE).unwrap_or(pack(30, 1));
        let fps = frames_per_second(rate).clamp(1.0, 60.0);

        let mut attributes = None;
        MFCreateAttributes(&mut attributes, 3).map_err(message)?;
        let attributes = attributes.ok_or("Media Foundation is missing")?;
        attributes.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1).map_err(message)?;
        attributes.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_FMPEG4).map_err(message)?;
        // Frames arrive at the camera's pace; the writer shouldn't hold the camera up.
        attributes.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1).map_err(message)?;
        let writer = MFCreateSinkWriterFromURL(&HSTRING::from(path), None, &attributes)
            .map_err(|e| format!("couldn't start the camera's file ({})", e.message()))?;
        // A face in a bubble: a tenth of a bit per pixel per frame is plenty.
        let bitrate = (f64::from(width) * f64::from(height) * fps * 0.1).clamp(4e6, 20e6) as u32;
        let output = MFCreateMediaType().map_err(message)?;
        output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(message)?;
        output.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264).map_err(message)?;
        output.SetUINT32(&MF_MT_AVG_BITRATE, bitrate).map_err(message)?;
        output.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32).map_err(message)?;
        output.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32).map_err(message)?;
        output.SetUINT64(&MF_MT_FRAME_SIZE, pack(width, height)).map_err(message)?;
        output.SetUINT64(&MF_MT_FRAME_RATE, rate).map_err(message)?;
        output.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1)).map_err(message)?;
        let refused =
            |e: windows::core::Error| format!("this computer can't encode the camera ({})", e.message());
        let stream = writer.AddStream(&output).map_err(refused)?;
        writer.SetInputMediaType(stream, &format, None).map_err(refused)?;
        writer.BeginWriting().map_err(refused)?;
        Ok((reader, writer, stream))
    }
}

/// Writes frames until told to stop or the camera ends.
fn film(reader: &IMFSourceReader, writer: &IMFSinkWriter, stream: u32, stop: &AtomicBool) -> Filmed {
    // The camera's time for its first frame, and the frames' clock when it arrived.
    let mut first: Option<(i64, f64)> = None;
    let mut end = 0i64;
    while !stop.load(Ordering::Relaxed) {
        let (mut flags, mut time, mut sample) = (0u32, 0i64, None);
        // SAFETY: the out-pointers are valid for the call, which waits for the camera's next frame.
        unsafe {
            reader.ReadSample(
                VIDEO,
                0,
                None,
                Some(&raw mut flags),
                Some(&raw mut time),
                Some(&raw mut sample),
            )
        }
        .map_err(|e| format!("the camera stopped ({})", e.message()))?;
        if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 {
            return Err("the camera stopped".into());
        }
        if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
            break;
        }
        let Some(sample) = sample else { continue };
        let (base, _) = *first.get_or_insert_with(|| (time, clock::now()));
        let at = time - base;
        if at < 0 {
            continue;
        }
        // SAFETY: re-times a frame the reader made in the writer's input format, then hands it over.
        unsafe {
            let duration = sample.GetSampleDuration().unwrap_or(333_333).max(1);
            sample.SetSampleTime(at).map_err(message)?;
            writer
                .WriteSample(stream, &sample)
                .map_err(|e| format!("a camera frame couldn't be encoded ({})", e.message()))?;
            end = end.max(at + duration);
        }
    }
    Ok(first.map(|(taken, arrived)| {
        // Cameras time their frames on the performance counter, as the screen does. One that doesn't is placed by
        // when its first frame arrived.
        let taken = taken as f64 / 1e7;
        let start = if (arrived - taken).abs() < 0.5 { taken } else { arrived };
        (start, start + end as f64 / 1e7)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_cameras_with_the_first_as_the_default() {
        let found = cameras();
        assert!(found.iter().skip(1).all(|c| !c.is_default));
        assert_eq!(found.first().map(|c| c.is_default), found.first().map(|_| true));
        eprintln!("{} camera(s)", found.len());
    }

    /// Films the first camera for two seconds. It turns the camera on, so it only runs when asked for.
    #[test]
    #[ignore = "films with the camera"]
    fn films_the_first_camera() {
        if cameras().is_empty() {
            eprintln!("skipped: there's no camera");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("camera.mp4");
        let recorder = CameraRecorder::start(None, &path).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(2));
        let (start, end) = recorder.stop().unwrap().expect("no frame came from the camera");
        assert!(end - start > 1.0, "{start} to {end}");
        assert!((clock::now() - start) < 10.0, "the first frame isn't on the frames' clock: {start}");
        assert!(std::fs::metadata(&path).unwrap().len() > 10_000);
    }
}
