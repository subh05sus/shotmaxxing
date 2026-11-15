//! Recording a display or a window. Windows Graphics Capture hands over each changed frame as a Direct3D texture,
//! which goes straight to Media Foundation's H.264 encoder (the graphics card's, where there is one) and into a
//! fragmented MP4 that stays playable even if the app stops suddenly. Nothing passes through the CPU.
//!
//! A still screen sends no frames, so the frame rate follows what changes. Each frame is held until the next one
//! arrives, so it lasts exactly as long as it was on screen; the last one lasts until the recording stops. The real
//! pointer is left out of the picture: the telemetry records it, and the editor draws it.

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_CREATE_DEVICE_FLAG, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_DEFAULT, D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread,
    ID3D11RenderTargetView, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{IDXGIAdapter, IDXGIDevice};
use windows::Win32::Media::MediaFoundation::{
    IMF2DBuffer, IMFDXGIDeviceManager, IMFSinkWriter, MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
    MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_MPEG2_PROFILE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE,
    MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, MF_SINK_WRITER_D3D_MANAGER, MF_SINK_WRITER_DISABLE_THROTTLING,
    MF_TRANSCODE_CONTAINERTYPE, MF_VERSION, MFCreateAttributes, MFCreateDXGIDeviceManager,
    MFCreateDXGISurfaceBuffer, MFCreateMediaType, MFCreateSample, MFCreateSinkWriterFromURL,
    MFMediaType_Video, MFSTARTUP_FULL, MFStartup, MFTranscodeContainerType_FMPEG4, MFVideoFormat_ARGB32,
    MFVideoFormat_H264, MFVideoInterlace_Progressive, eAVEncH264VProfile_High,
};
use windows::Win32::System::Com::CoIncrementMTAUsage;
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::core::{Interface, PCWSTR};

use super::{clock, dpi_aware, input, monitor, wide, window_bounds};
use crate::{CaptureError, RecordOptions, Recording, Target};

fn system(error: windows::core::Error) -> CaptureError {
    CaptureError::System(error.message())
}

/// COM for worker threads that have none of their own, and Media Foundation: once for the process.
fn start_systems() -> Result<(), CaptureError> {
    static STARTED: OnceLock<Result<(), String>> = OnceLock::new();
    STARTED
        .get_or_init(|| {
            // SAFETY: process-wide start-up calls without pointers. The apartment is kept for the process's life.
            unsafe {
                CoIncrementMTAUsage().map_err(|e| e.message())?;
                MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(|e| e.message())
            }
        })
        .clone()
        .map_err(CaptureError::System)
}

/// A Direct3D device that capture, the encoder and this code share across their threads: the graphics card's, or
/// Windows' software one when there's no card to use.
fn device() -> Result<ID3D11Device, CaptureError> {
    let try_kind = |kind: D3D_DRIVER_TYPE, flags: D3D11_CREATE_DEVICE_FLAG| {
        let mut device = None;
        // SAFETY: creates a device into `device`; no adapter, module or feature list is passed.
        unsafe {
            D3D11CreateDevice(
                None::<&IDXGIAdapter>,
                kind,
                HMODULE::default(),
                flags,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                None,
            )
        }
        .ok()
        .and(device)
    };
    let video = D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT;
    let device = try_kind(D3D_DRIVER_TYPE_HARDWARE, video)
        .or_else(|| try_kind(D3D_DRIVER_TYPE_WARP, video))
        .or_else(|| try_kind(D3D_DRIVER_TYPE_WARP, D3D11_CREATE_DEVICE_BGRA_SUPPORT))
        .ok_or_else(|| CaptureError::System("Direct3D isn't available".into()))?;
    let multithread: ID3D11Multithread = device.cast().map_err(system)?;
    // SAFETY: turns on the device's own locking, as Media Foundation requires for a shared device.
    let _ = unsafe { multithread.SetMultithreadProtected(true) };
    Ok(device)
}

fn texture(device: &ID3D11Device, width: u32, height: u32) -> windows::core::Result<ID3D11Texture2D> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let mut texture = None;
    // SAFETY: creates one texture from a complete description, without initial data.
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture)) }?;
    texture.ok_or_else(|| windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL))
}

const fn pack(high: u32, low: u32) -> u64 {
    ((high as u64) << 32) | low as u64
}

/// Media Foundation's sink writer, encoding textures into H.264 in a fragmented MP4.
struct Encoder {
    writer: IMFSinkWriter,
    stream: u32,
    /// Keeps the encoder's view of the device alive.
    _manager: IMFDXGIDeviceManager,
}

impl Encoder {
    fn new(
        device: &ID3D11Device,
        path: &Path,
        width: u32,
        height: u32,
        fps: u32,
    ) -> windows::core::Result<Self> {
        // Screen pictures are sharp and mostly still: about a tenth of a bit per pixel per frame, within limits.
        let bitrate = (f64::from(width) * f64::from(height) * f64::from(fps) * 0.1).clamp(4e6, 80e6) as u32;
        // SAFETY: Media Foundation calls with valid objects and NUL-terminated strings that outlive them.
        unsafe {
            let mut token = 0;
            let mut manager = None;
            MFCreateDXGIDeviceManager(&mut token, &mut manager)?;
            let manager = manager
                .ok_or_else(|| windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL))?;
            manager.ResetDevice(device, token)?;

            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 4)?;
            let attributes = attributes
                .ok_or_else(|| windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL))?;
            attributes.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
            attributes.SetUnknown(&MF_SINK_WRITER_D3D_MANAGER, &manager)?;
            attributes.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_FMPEG4)?;
            // Frames arrive as the screen changes; the writer shouldn't hold capture up.
            attributes.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)?;
            let url = wide(path.as_os_str());
            let writer = MFCreateSinkWriterFromURL(PCWSTR(url.as_ptr()), None, &attributes)?;

            let output = MFCreateMediaType()?;
            output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            output.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
            output.SetUINT32(&MF_MT_AVG_BITRATE, bitrate)?;
            output.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32)?;
            output.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            output.SetUINT64(&MF_MT_FRAME_SIZE, pack(width, height))?;
            output.SetUINT64(&MF_MT_FRAME_RATE, pack(fps, 1))?;
            output.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
            let stream = writer.AddStream(&output)?;

            // Capture's BGRA textures go in as they are; the writer converts them on the graphics card.
            let input = MFCreateMediaType()?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_ARGB32)?;
            input.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            input.SetUINT64(&MF_MT_FRAME_SIZE, pack(width, height))?;
            input.SetUINT64(&MF_MT_FRAME_RATE, pack(fps, 1))?;
            input.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
            writer.SetInputMediaType(stream, &input, None)?;
            writer.BeginWriting()?;
            Ok(Self { writer, stream, _manager: manager })
        }
    }

    /// Writes one frame: `at` and `duration` in 100-nanosecond units from the recording's start.
    fn write(&self, texture: &ID3D11Texture2D, at: i64, duration: i64) -> windows::core::Result<()> {
        // SAFETY: wraps a live texture in a buffer that keeps it alive until the encoder is done with it.
        unsafe {
            let buffer = MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, texture, 0, false)?;
            // The writer only takes a buffer that says how much it holds.
            let length = buffer.cast::<IMF2DBuffer>()?.GetContiguousLength()?;
            buffer.SetCurrentLength(length)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(at)?;
            sample.SetSampleDuration(duration.max(1))?;
            self.writer.WriteSample(self.stream, &sample)
        }
    }

    fn finish(self) -> windows::core::Result<()> {
        // SAFETY: finishes the file; the writer isn't used again.
        unsafe { self.writer.Finalize() }
    }
}

/// What the frame callback and the recorder share.
struct Shared {
    state: Mutex<State>,
    frames: AtomicU64,
    /// The window being recorded was closed.
    closed: AtomicBool,
}

struct State {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    encoder: Option<Encoder>,
    width: u32,
    height: u32,
    /// The captured picture's size, to follow a window that's resized.
    content: (i32, i32),
    /// Shortest time between frames, from the frame-rate cap (100-ns units).
    gap: i64,
    /// The first frame's time on the system clock, in seconds.
    first: Option<f64>,
    /// The frame waiting for the next one, which decides how long it lasts.
    held: Option<(ID3D11Texture2D, i64)>,
    error: Option<String>,
}

// SAFETY: the Direct3D device is multithread protected and Media Foundation's sink writer is free-threaded; every
// use goes through the mutex that holds this state.
unsafe impl Send for State {}

impl State {
    /// Writes the held frame, lasting until `until` (100-ns units from the start).
    fn release(&mut self, until: i64) {
        let (Some(encoder), Some((texture, at))) = (self.encoder.as_ref(), self.held.take()) else { return };
        if let Err(error) = encoder.write(&texture, at, until - at) {
            tracing::warn!(%error, "a frame couldn't be saved");
            self.error.get_or_insert_with(|| error.message());
        }
    }
}

fn on_frame(shared: &Shared, pool: &Direct3D11CaptureFramePool) -> windows::core::Result<()> {
    let frame = pool.TryGetNextFrame()?;
    let time = frame.SystemRelativeTime()?.Duration as f64 / 1e7;
    let Ok(mut state) = shared.state.lock() else { return frame.Close() };
    if state.encoder.is_none() {
        return frame.Close();
    }
    let first = *state.first.get_or_insert(time);
    let at = ((time - first) * 1e7).round() as i64;
    if state.held.as_ref().is_some_and(|(_, held)| at - held < state.gap) {
        // Faster than the frame-rate cap: this change shows in the next frame.
        return frame.Close();
    }
    let size = frame.ContentSize()?;
    let source: ID3D11Texture2D =
        // SAFETY: the frame's surface is a Direct3D texture on this device while the frame is open.
        unsafe { frame.Surface()?.cast::<IDirect3DDxgiInterfaceAccess>()?.GetInterface()? };
    let copy = texture(&state.device, state.width, state.height)?;
    let (width, height) =
        ((size.Width.max(0) as u32).min(state.width), (size.Height.max(0) as u32).min(state.height));
    // SAFETY: copies inside both textures' bounds on the shared, locked device.
    unsafe {
        if width < state.width || height < state.height {
            // A window that shrank leaves black around it rather than old pixels.
            let mut view: Option<ID3D11RenderTargetView> = None;
            state.device.CreateRenderTargetView(&copy, None, Some(&mut view))?;
            if let Some(view) = view {
                state.context.ClearRenderTargetView(&view, &[0.0, 0.0, 0.0, 1.0]);
            }
        }
        let region = D3D11_BOX { left: 0, top: 0, front: 0, right: width, bottom: height, back: 1 };
        state.context.CopySubresourceRegion(&copy, 0, 0, 0, 0, &source, 0, Some(&region));
    }
    frame.Close()?;
    state.release(at);
    state.held = Some((copy, at));
    shared.frames.fetch_add(1, Ordering::Relaxed);
    // Follow a resized window: the pool makes frames at the new size from now on.
    if (size.Width, size.Height) != state.content && size.Width > 0 && size.Height > 0 {
        state.content = (size.Width, size.Height);
        drop(state);
        if let Ok(device) = direct3d(shared) {
            let _ = pool.Recreate(&device, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2, size);
        }
    }
    Ok(())
}

/// The shared device as capture takes it.
fn direct3d(shared: &Shared) -> windows::core::Result<IDirect3DDevice> {
    let device = shared
        .state
        .lock()
        .map(|state| state.device.clone())
        .map_err(|_| windows::core::Error::from_hresult(windows::Win32::Foundation::E_FAIL))?;
    winrt_device(&device)
}

fn winrt_device(device: &ID3D11Device) -> windows::core::Result<IDirect3DDevice> {
    let dxgi: IDXGIDevice = device.cast()?;
    // SAFETY: wraps a live DXGI device for WinRT.
    unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }?.cast()
}

pub struct Recorder {
    session: GraphicsCaptureSession,
    pool: Direct3D11CaptureFramePool,
    _item: GraphicsCaptureItem,
    shared: Arc<Shared>,
    started: f64,
    output: PathBuf,
    size: (f64, f64),
    scale: f64,
    app_name: Option<String>,
    window_title: Option<String>,
    warnings: Vec<String>,
    input: Option<input::Telemetry>,
}

impl Recorder {
    pub fn start(options: RecordOptions) -> Result<Self, CaptureError> {
        dpi_aware();
        start_systems()?;
        if !GraphicsCaptureSession::IsSupported().unwrap_or(false) {
            return Err(CaptureError::Unsupported);
        }
        let interop =
            windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>().map_err(system)?;
        let (item, scale, origin, app_name, window_title) = match options.target {
            Target::Display(number) => {
                let monitor = monitor(number)?;
                // SAFETY: a monitor handle Windows just listed.
                let item: GraphicsCaptureItem =
                    unsafe { interop.CreateForMonitor(monitor.handle) }.map_err(system)?;
                (item, monitor.scale, (monitor.bounds.left, monitor.bounds.top), None, None)
            }
            Target::Window(id) => {
                let window = HWND(id as usize as *mut c_void);
                let Some(bounds) = window_bounds(window) else {
                    return Err(CaptureError::SourceNotFound(format!("window {id}")));
                };
                let info = super::sources()?.windows.into_iter().find(|w| w.id == id);
                // SAFETY: a window handle that answered just now; capture fails cleanly if it has closed since.
                let item: GraphicsCaptureItem = unsafe { interop.CreateForWindow(window) }
                    .map_err(|_| CaptureError::SourceNotFound(format!("window {id}")))?;
                // SAFETY: asks the window's DPI; a closed window answers 0, which falls back to 96.
                let scale = f64::from(unsafe { GetDpiForWindow(window) }.max(48)) / 96.0;
                let (app, title) = info.map(|w| (w.app, w.title)).unzip();
                (item, scale, (bounds.left, bounds.top), app, title)
            }
        };
        let size = item.Size().map_err(system)?;
        // 4:2:0 video needs even sides.
        let (width, height) = ((size.Width.max(2) as u32) & !1, (size.Height.max(2) as u32) & !1);
        let device = device()?;
        // SAFETY: the device's own immediate context.
        let context = unsafe { device.GetImmediateContext() }.map_err(system)?;
        let fps = options.max_fps.clamp(1, 120);
        if let Some(parent) = options.output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let encoder = Encoder::new(&device, &options.output, width, height, fps).map_err(system)?;

        let mut warnings = Vec::new();
        for (asked, what) in [
            (options.system_audio.is_some(), "The computer's sound"),
            (options.microphone.is_some(), "The microphone"),
            (options.camera.is_some(), "The camera"),
        ] {
            if asked {
                warnings.push(format!("{what} isn't recorded on Windows yet."));
            }
        }

        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                device: device.clone(),
                context,
                encoder: Some(encoder),
                width,
                height,
                content: (size.Width, size.Height),
                gap: (1e7 / f64::from(fps) * 0.9) as i64,
                first: None,
                held: None,
                error: None,
            }),
            frames: AtomicU64::new(0),
            closed: AtomicBool::new(false),
        });
        let direct3d = winrt_device(&device).map_err(system)?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &direct3d,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2,
            size,
        )
        .map_err(system)?;
        let frames = shared.clone();
        pool.FrameArrived(&TypedEventHandler::new(
            move |pool: windows::core::Ref<Direct3D11CaptureFramePool>, _| on_frame(&frames, pool.ok()?),
        ))
        .map_err(system)?;
        let closed = shared.clone();
        item.Closed(&TypedEventHandler::new(move |_, _| {
            closed.closed.store(true, Ordering::SeqCst);
            Ok(())
        }))
        .map_err(system)?;
        let session = pool.CreateCaptureSession(&item).map_err(system)?;
        // The pointer is recorded as data and drawn by the editor, so it stays out of the picture.
        if session.SetIsCursorCaptureEnabled(false).is_err() {
            warnings.push("This version of Windows draws the pointer into the recording.".into());
        }
        // Windows 11 can leave out the yellow frame around what's recorded.
        let _ = session.SetIsBorderRequired(false);
        let started = clock::now();
        session.StartCapture().map_err(system)?;
        let input = options.telemetry.then(|| input::Telemetry::start(origin, scale));
        Ok(Self {
            session,
            pool,
            _item: item,
            shared,
            started,
            output: options.output,
            size: (f64::from(width) / scale, f64::from(height) / scale),
            scale,
            app_name,
            window_title,
            warnings,
            input,
        })
    }

    pub fn elapsed(&self) -> f64 {
        clock::now() - self.started
    }

    pub fn frames(&self) -> u64 {
        self.shared.frames.load(Ordering::Relaxed)
    }

    pub fn stop(mut self) -> Result<Recording, CaptureError> {
        let stopped = clock::now();
        let _ = self.session.Close();
        let _ = self.pool.Close();
        let mut state =
            self.shared.state.lock().map_err(|_| CaptureError::System("capture stopped badly".into()))?;
        let first = state.first.unwrap_or(self.started);
        // The last picture stays up until the recording stops.
        let end = ((stopped - first).max(0.0) * 1e7).round() as i64;
        state.release(end);
        let encoder = state
            .encoder
            .take()
            .ok_or_else(|| CaptureError::System("the recording was already finished".into()))?;
        if let Some(error) = state.error.take() {
            self.warnings.push(format!("Some frames couldn't be saved: {error}"));
        }
        drop(state);
        encoder.finish().map_err(system)?;
        if self.shared.closed.load(Ordering::SeqCst) {
            self.warnings.push("The window closed during the recording.".into());
        }
        let (cursor, events) = self.input.take().map(|input| input.stop(first)).unwrap_or_default();
        Ok(Recording {
            output: self.output,
            duration_us: (end / 10).max(0) as u64,
            frames: self.shared.frames.load(Ordering::Relaxed),
            dropped: 0,
            width: self.size.0,
            height: self.size.1,
            scale: self.scale,
            app_name: self.app_name,
            window_title: self.window_title,
            cursor,
            events,
            system_audio: None,
            microphone: None,
            camera: None,
            warnings: self.warnings,
        })
    }
}

// The recorder moves between the app's threads.
const _: () = {
    const fn sendable<T: Send>() {}
    sendable::<Recorder>();
};
