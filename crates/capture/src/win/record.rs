//! Recording a display or a window. Windows Graphics Capture hands over each changed frame as a Direct3D texture.
//! The capture callback only copies it into one of a few textures kept for that and hands it on: it never waits
//! for the encoder. An encoding thread turns the picture into NV12 on the graphics card (Direct3D's video
//! processor, which also scales it down when the encoder couldn't keep up with its full size) and gives it to Media
//! Foundation's H.264 encoder (the graphics card's, where there is one), writing a fragmented MP4 that stays playable
//! even if the app stops suddenly. Nothing passes through the CPU.
//!
//! A still screen sends no frames, so the frame rate follows what changes. Each frame is held until the next one
//! arrives, so it lasts exactly as long as it was on screen; the last one lasts until the recording stops. When the
//! encoder falls behind, the newest picture wins: the ones it skips are counted as dropped, never queued. The real
//! pointer is left out of the picture: the telemetry records it, and the editor draws it.

use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread::JoinHandle;

use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::{E_FAIL, E_NOTIMPL, HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_CREATE_DEVICE_FLAG, D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION, D3D11_TEX2D_ARRAY_VPOV,
    D3D11_TEX2D_VPIV, D3D11_TEX2D_VPOV, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
    D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE, D3D11_VIDEO_PROCESSOR_COLOR_SPACE,
    D3D11_VIDEO_PROCESSOR_CONTENT_DESC, D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT,
    D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_STREAM, D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
    D3D11_VPIV_DIMENSION_TEXTURE2D, D3D11_VPOV_DIMENSION_TEXTURE2D, D3D11_VPOV_DIMENSION_TEXTURE2DARRAY,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread, ID3D11RenderTargetView,
    ID3D11Texture2D, ID3D11VideoContext, ID3D11VideoDevice, ID3D11VideoProcessor,
    ID3D11VideoProcessorEnumerator, ID3D11VideoProcessorInputView, ID3D11VideoProcessorOutputView,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12, DXGI_RATIONAL, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{IDXGIAdapter, IDXGIDevice};
use windows::Win32::Media::MediaFoundation::{
    IMF2DBuffer, IMFActivate, IMFAttributes, IMFDXGIBuffer, IMFDXGIDeviceManager, IMFMediaType, IMFSample,
    IMFSinkWriter, IMFSinkWriterEx, IMFVideoSampleAllocatorEx, MF_E_SAMPLEALLOCATOR_EMPTY, MF_MT_AVG_BITRATE,
    MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_MPEG2_PROFILE,
    MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_MT_VIDEO_NOMINAL_RANGE, MF_MT_YUV_MATRIX,
    MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, MF_SA_D3D11_BINDFLAGS, MF_SINK_WRITER_D3D_MANAGER,
    MF_SINK_WRITER_DISABLE_THROTTLING, MF_TRANSCODE_CONTAINERTYPE, MF_VERSION, MFCreateAttributes,
    MFCreateDXGIDeviceManager, MFCreateDXGISurfaceBuffer, MFCreateMediaType, MFCreateSample,
    MFCreateSinkWriterFromURL, MFCreateVideoSampleAllocatorEx, MFMediaType_Video, MFNominalRange_16_235,
    MFSTARTUP_FULL, MFStartup, MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG, MFT_ENUM_FLAG_HARDWARE,
    MFT_ENUM_FLAG_SORTANDFILTER, MFT_ENUM_HARDWARE_URL_Attribute, MFT_FRIENDLY_NAME_Attribute,
    MFT_REGISTER_TYPE_INFO, MFTEnumEx, MFTranscodeContainerType_FMPEG4, MFVideoFormat_ARGB32,
    MFVideoFormat_H264, MFVideoFormat_NV12, MFVideoInterlace_Progressive, MFVideoTransferMatrix_BT709,
    eAVEncH264VProfile_High,
};
use windows::Win32::System::Com::{CoIncrementMTAUsage, CoTaskMemFree};
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::core::{GUID, Interface, PCWSTR, PWSTR};

use super::audio::{AudioRecorder, Source};
use super::camera::CameraRecorder;
use super::{clock, dpi_aware, input, monitor, wide, window_bounds};
use crate::{CaptureError, RecordOptions, RecordedTrack, Recording, Target};

fn system(error: windows::core::Error) -> CaptureError {
    CaptureError::System(error.message())
}

fn failed() -> windows::core::Error {
    windows::core::Error::from_hresult(E_FAIL)
}

/// Locks a mutex even if a thread panicked while holding it: what it guards stays usable.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// How a track recorded beside the screen ended (its start and end on the frames' clock, if anything came),
/// and its file.
type Ended = (Result<Option<(f64, f64)>, String>, PathBuf);

/// Textures the capture callback copies frames into: one being converted, one waiting, one being filled, a spare.
const SLOTS: usize = 4;
/// NV12 pictures out with the encoder at most. When all are out, the encoder is behind and a frame is dropped.
const SAMPLES: u32 = 8;
/// The most pixels an encoder is given at once: the graphics card's takes 4K at 60 frames a second, Windows' own
/// software encoder about 1080p.
const HARDWARE_PIXELS: u64 = 3840 * 2160;
const SOFTWARE_PIXELS: u64 = 1920 * 1080;
/// The longest side H.264 (level 5.2) allows.
const LONGEST_SIDE: u32 = 4096;

/// COM for worker threads that have none of their own, and Media Foundation: once for the process.
pub(super) fn start_systems() -> Result<(), CaptureError> {
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
    texture.ok_or_else(failed)
}

const fn pack(high: u32, low: u32) -> u64 {
    ((high as u64) << 32) | low as u64
}

/// The size to encode a `width`×`height` capture at: its own, or the same shape scaled down (with even sides) to at
/// most `most_pixels` and H.264's longest side.
fn encode_size(width: u32, height: u32, most_pixels: u64) -> (u32, u32) {
    let pixels = u64::from(width) * u64::from(height);
    let by_area = if pixels > most_pixels { (most_pixels as f64 / pixels as f64).sqrt() } else { 1.0 };
    let factor = by_area.min(f64::from(LONGEST_SIDE) / f64::from(width.max(height).max(1)));
    if factor >= 1.0 {
        return (width, height);
    }
    let side = |length: u32| (((f64::from(length) * factor) as u32) & !1).max(2);
    (side(width), side(height))
}

/// An attribute store's friendly name for a transform, if it has one.
fn friendly_name(attributes: &IMFAttributes) -> Option<String> {
    let (mut text, mut length) = (PWSTR::null(), 0u32);
    // SAFETY: Media Foundation allocates the string; it's read once and freed with the allocator it came from.
    unsafe {
        attributes.GetAllocatedString(&MFT_FRIENDLY_NAME_Attribute, &mut text, &mut length).ok()?;
        let name = text.to_string().ok();
        CoTaskMemFree(Some(text.0 as *const c_void));
        name
    }
}

/// The name of the graphics card's H.264 encoder that takes NV12, if the computer has one.
fn hardware_encoder() -> Option<String> {
    let input = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: MFVideoFormat_NV12 };
    let output = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: MFVideoFormat_H264 };
    let (mut list, mut count) = (std::ptr::null_mut::<Option<IMFActivate>>(), 0u32);
    let flags = MFT_ENUM_FLAG(MFT_ENUM_FLAG_HARDWARE.0 | MFT_ENUM_FLAG_SORTANDFILTER.0);
    // SAFETY: Media Foundation fills `list` with `count` activation objects in memory it allocated; each is released
    // and the array is freed with the allocator it came from.
    unsafe {
        MFTEnumEx(MFT_CATEGORY_VIDEO_ENCODER, flags, Some(&input), Some(&output), &mut list, &mut count)
            .ok()?;
        if list.is_null() {
            return None;
        }
        let found = std::slice::from_raw_parts_mut(list, count as usize);
        let name = found
            .first()
            .and_then(Option::as_ref)
            .map(|first| friendly_name(first).unwrap_or_else(|| "a hardware encoder".into()));
        for activate in found.iter_mut() {
            drop(activate.take());
        }
        CoTaskMemFree(Some(list as *const c_void));
        name
    }
}

/// Media Foundation's sink writer, encoding pictures into H.264 in a fragmented MP4.
struct Encoder {
    writer: IMFSinkWriter,
    stream: u32,
    /// Hands out NV12 pictures on the graphics card and takes each back once the encoder is done with it. `None` when
    /// the writer is given BGRA and converts it itself.
    samples: Option<IMFVideoSampleAllocatorEx>,
    /// Keeps the encoder's view of the device alive.
    _manager: IMFDXGIDeviceManager,
}

/// A video media type of `subtype` at `width`×`height` and `fps`, progressive with square pixels.
fn video_type(subtype: &GUID, (width, height): (u32, u32), fps: u32) -> windows::core::Result<IMFMediaType> {
    // SAFETY: sets attributes on a media type this function owns.
    unsafe {
        let kind = MFCreateMediaType()?;
        kind.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        kind.SetGUID(&MF_MT_SUBTYPE, subtype)?;
        kind.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        kind.SetUINT64(&MF_MT_FRAME_SIZE, pack(width, height))?;
        kind.SetUINT64(&MF_MT_FRAME_RATE, pack(fps, 1))?;
        kind.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
        Ok(kind)
    }
}

/// Marks a type's colours as HD video's: BT.709, in the 16–235 range the video processor writes.
fn hd_colours(kind: &IMFMediaType) -> windows::core::Result<()> {
    // SAFETY: sets attributes on a live media type.
    unsafe {
        kind.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)?;
        kind.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)
    }
}

impl Encoder {
    /// A writer at `path` taking `size` pictures: NV12 ones from its own pool when `nv12`, else BGRA textures.
    fn new(
        device: &ID3D11Device,
        path: &Path,
        size: (u32, u32),
        fps: u32,
        nv12: bool,
    ) -> windows::core::Result<Self> {
        let (width, height) = size;
        // Screen pictures are sharp and mostly still: about a tenth of a bit per pixel per frame, within limits.
        let bitrate = (f64::from(width) * f64::from(height) * f64::from(fps) * 0.1).clamp(4e6, 80e6) as u32;
        // SAFETY: Media Foundation calls with valid objects and NUL-terminated strings that outlive them.
        unsafe {
            let mut token = 0;
            let mut manager = None;
            MFCreateDXGIDeviceManager(&mut token, &mut manager)?;
            let manager = manager.ok_or_else(failed)?;
            manager.ResetDevice(device, token)?;

            let input = if nv12 {
                let input = video_type(&MFVideoFormat_NV12, size, fps)?;
                hd_colours(&input)?;
                input
            } else {
                // Capture's BGRA textures go in as they are; the writer converts them.
                video_type(&MFVideoFormat_ARGB32, size, fps)?
            };
            // The pool comes before the file, so a computer that can't make one leaves no file behind.
            let samples = if nv12 {
                let mut raw = std::ptr::null_mut();
                MFCreateVideoSampleAllocatorEx(&IMFVideoSampleAllocatorEx::IID, &mut raw)?;
                let samples = IMFVideoSampleAllocatorEx::from_raw(raw);
                samples.SetDirectXManager(&manager)?;
                let mut attributes = None;
                MFCreateAttributes(&mut attributes, 1)?;
                let attributes = attributes.ok_or_else(failed)?;
                // The video processor draws into them.
                attributes.SetUINT32(&MF_SA_D3D11_BINDFLAGS, D3D11_BIND_RENDER_TARGET.0 as u32)?;
                samples.InitializeSampleAllocatorEx(SAMPLES / 2, SAMPLES, &attributes, &input)?;
                Some(samples)
            } else {
                None
            };

            let mut attributes = None;
            MFCreateAttributes(&mut attributes, 4)?;
            let attributes = attributes.ok_or_else(failed)?;
            attributes.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
            attributes.SetUnknown(&MF_SINK_WRITER_D3D_MANAGER, &manager)?;
            attributes.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_FMPEG4)?;
            // Frames arrive as the screen changes; the writer shouldn't hold the encoding thread up. The pool above
            // bounds how many are out at once.
            attributes.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)?;
            let url = wide(path.as_os_str());
            let writer = MFCreateSinkWriterFromURL(PCWSTR(url.as_ptr()), None, &attributes)?;

            let output = video_type(&MFVideoFormat_H264, size, fps)?;
            output.SetUINT32(&MF_MT_AVG_BITRATE, bitrate)?;
            output.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32)?;
            if nv12 {
                hd_colours(&output)?;
            }
            let stream = writer.AddStream(&output)?;
            writer.SetInputMediaType(stream, &input, None)?;
            writer.BeginWriting()?;
            Ok(Self { writer, stream, samples, _manager: manager })
        }
    }

    /// The transforms the writer put between the pictures and the file, by name, the encoder marked with whether
    /// it runs on the graphics card.
    fn transforms(&self) -> Vec<String> {
        let Ok(writer) = self.writer.cast::<IMFSinkWriterEx>() else { return Vec::new() };
        let mut names = Vec::new();
        for index in 0..8 {
            let (mut category, mut transform) = (GUID::zeroed(), None);
            // SAFETY: asks for one transform of this writer's stream into live out-pointers.
            if unsafe {
                writer.GetTransformForStream(self.stream, index, Some(&mut category), &mut transform)
            }
            .is_err()
            {
                break;
            }
            // SAFETY: reads a live transform's attributes.
            let Some(attributes) = transform.and_then(|t| unsafe { t.GetAttributes() }.ok()) else {
                continue;
            };
            let name = friendly_name(&attributes).unwrap_or_else(|| "a transform".into());
            if category == MFT_CATEGORY_VIDEO_ENCODER {
                // SAFETY: reads one attribute's length; hardware transforms carry this one.
                let hardware =
                    unsafe { attributes.GetStringLength(&MFT_ENUM_HARDWARE_URL_Attribute) }.is_ok();
                names.push(format!("{name} ({})", if hardware { "graphics card" } else { "software" }));
            } else {
                names.push(name);
            }
        }
        names
    }

    /// Writes one picture: `at` and `duration` in 100-nanosecond units from the recording's start.
    fn write(&self, sample: &IMFSample, at: i64, duration: i64) -> windows::core::Result<()> {
        // SAFETY: a live sample handed to the writer, which keeps what it needs.
        unsafe {
            sample.SetSampleTime(at)?;
            sample.SetSampleDuration(duration.max(1))?;
            self.writer.WriteSample(self.stream, sample)
        }
    }

    fn finish(self) -> windows::core::Result<()> {
        // SAFETY: finishes the file; the writer isn't used again.
        unsafe { self.writer.Finalize() }
    }
}

/// Wraps a texture in a sample whose buffer keeps it alive until the encoder is done with it.
fn sample_of(texture: &ID3D11Texture2D) -> windows::core::Result<IMFSample> {
    // SAFETY: Media Foundation calls on live objects.
    unsafe {
        let buffer = MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, texture, 0, false)?;
        // The writer only takes a buffer that says how much it holds.
        let length = buffer.cast::<IMF2DBuffer>()?.GetContiguousLength()?;
        buffer.SetCurrentLength(length)?;
        let sample = MFCreateSample()?;
        sample.AddBuffer(&buffer)?;
        Ok(sample)
    }
}

/// Direct3D's video processor, turning the captured BGRA pictures into NV12 at the encoder's size.
struct Converter {
    video: ID3D11VideoDevice,
    context: ID3D11VideoContext,
    enumerator: ID3D11VideoProcessorEnumerator,
    processor: ID3D11VideoProcessor,
    /// The processor's view of each slot.
    inputs: Vec<ID3D11VideoProcessorInputView>,
}

impl Converter {
    fn new(
        gpu: &Gpu,
        (width, height): (u32, u32),
        (out_width, out_height): (u32, u32),
        fps: u32,
    ) -> windows::core::Result<Self> {
        let video: ID3D11VideoDevice = gpu.device.cast()?;
        let context: ID3D11VideoContext = gpu.context.cast()?;
        let rate = DXGI_RATIONAL { Numerator: fps, Denominator: 1 };
        let desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: rate,
            InputWidth: width,
            InputHeight: height,
            OutputFrameRate: rate,
            OutputWidth: out_width,
            OutputHeight: out_height,
            Usage: D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
        };
        // SAFETY: Direct3D video calls on live objects with complete descriptions; the views' resources are this
        // recorder's own slots.
        unsafe {
            let enumerator = video.CreateVideoProcessorEnumerator(&desc)?;
            let input = enumerator.CheckVideoProcessorFormat(DXGI_FORMAT_B8G8R8A8_UNORM)?;
            let output = enumerator.CheckVideoProcessorFormat(DXGI_FORMAT_NV12)?;
            if input & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT.0 as u32 == 0
                || output & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT.0 as u32 == 0
            {
                return Err(windows::core::Error::from_hresult(E_NOTIMPL));
            }
            let processor = video.CreateVideoProcessor(&enumerator, 0)?;
            // In: full-range RGB. Out: BT.709 (bit 2) in the 16–235 range (bits 4–5 = 1), as HD video is.
            context.VideoProcessorSetStreamColorSpace(
                &processor,
                0,
                &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: 0 },
            );
            context.VideoProcessorSetOutputColorSpace(
                &processor,
                &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: (1 << 2) | (1 << 4) },
            );
            context.VideoProcessorSetStreamFrameFormat(&processor, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
            // No denoising or sharpening of its own: the screen as it was.
            context.VideoProcessorSetStreamAutoProcessingMode(&processor, 0, false);
            let view = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                FourCC: 0,
                ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPIV { MipSlice: 0, ArraySlice: 0 },
                },
            };
            let mut inputs = Vec::with_capacity(gpu.slots.len());
            for slot in &gpu.slots {
                let mut input = None;
                video.CreateVideoProcessorInputView(slot, &enumerator, &view, Some(&mut input))?;
                inputs.push(input.ok_or_else(failed)?);
            }
            Ok(Self { video, context, enumerator, processor, inputs })
        }
    }

    /// Draws slot `slot` into an NV12 sample from the encoder's pool.
    fn convert(&self, slot: usize, sample: &IMFSample) -> windows::core::Result<()> {
        // SAFETY: the sample's buffer is a texture on this device; the views and the stream live through the call,
        // and the stream's own reference to the input view is released after it.
        unsafe {
            let buffer = sample.GetBufferByIndex(0)?;
            let dxgi: IMFDXGIBuffer = buffer.cast()?;
            let mut raw = std::ptr::null_mut();
            dxgi.GetResource(&ID3D11Texture2D::IID, &mut raw)?;
            let target = ID3D11Texture2D::from_raw(raw);
            let index = dxgi.GetSubresourceIndex()?;
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            target.GetDesc(&mut desc);
            let view = if desc.ArraySize > 1 {
                D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                    ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2DARRAY,
                    Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                        Texture2DArray: D3D11_TEX2D_ARRAY_VPOV {
                            MipSlice: 0,
                            FirstArraySlice: index,
                            ArraySize: 1,
                        },
                    },
                }
            } else {
                D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                    ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                    Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                        Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
                    },
                }
            };
            let mut output: Option<ID3D11VideoProcessorOutputView> = None;
            self.video.CreateVideoProcessorOutputView(&target, &self.enumerator, &view, Some(&mut output))?;
            let output = output.ok_or_else(failed)?;
            let mut streams = [D3D11_VIDEO_PROCESSOR_STREAM {
                Enable: true.into(),
                pInputSurface: ManuallyDrop::new(Some(self.inputs[slot].clone())),
                ..Default::default()
            }];
            let drawn = self.context.VideoProcessorBlt(&self.processor, &output, 0, &streams);
            ManuallyDrop::drop(&mut streams[0].pInputSurface);
            drawn?;
            let length = buffer.cast::<IMF2DBuffer>()?.GetContiguousLength()?;
            buffer.SetCurrentLength(length)
        }
    }
}

/// The device, and the textures frames are copied into. Everything drawn on the device goes through this lock.
struct Gpu {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    slots: Vec<ID3D11Texture2D>,
    width: u32,
    height: u32,
    /// The captured picture's size, to follow a window that's resized.
    content: (i32, i32),
}

// SAFETY: the Direct3D device is multithread protected, and every use of it here goes through the mutex that holds
// this.
unsafe impl Send for Gpu {}

/// What the capture callback hands the encoding thread.
struct Mailbox {
    /// Slots nobody is using.
    free: Vec<usize>,
    /// The newest frame not yet encoded: its slot and its time (100-ns units from the start).
    pending: Option<(usize, i64)>,
    /// The first frame's time on the system clock, in seconds.
    first: Option<f64>,
    /// The last frame taken, for the frame-rate cap.
    last: Option<i64>,
    /// Shortest time between frames, from the frame-rate cap (100-ns units).
    gap: i64,
    /// When the recording ended (100-ns units from the first frame): the encoding thread finishes the file.
    end: Option<i64>,
}

/// What the frame callback, the encoding thread and the recorder share.
struct Shared {
    gpu: Mutex<Gpu>,
    mailbox: Mutex<Mailbox>,
    /// Wakes the encoding thread for a frame or the end.
    ready: Condvar,
    frames: AtomicU64,
    dropped: AtomicU64,
    /// The window being recorded was closed.
    closed: AtomicBool,
}

fn on_frame(shared: &Shared, pool: &Direct3D11CaptureFramePool) -> windows::core::Result<()> {
    let frame = pool.TryGetNextFrame()?;
    let time = frame.SystemRelativeTime()?.Duration as f64 / 1e7;
    let (slot, at) = {
        let mut mail = lock(&shared.mailbox);
        if mail.end.is_some() {
            return frame.Close();
        }
        let first = *mail.first.get_or_insert(time);
        let at = ((time - first) * 1e7).round() as i64;
        if mail.last.is_some_and(|last| at - last < mail.gap) {
            // Faster than the frame-rate cap: this change shows in the next frame.
            return frame.Close();
        }
        let Some(slot) = mail.free.pop() else {
            shared.dropped.fetch_add(1, Ordering::Relaxed);
            return frame.Close();
        };
        mail.last = Some(at);
        (slot, at)
    };
    let size = frame.ContentSize()?;
    let copied = copy_frame(shared, &frame, slot, size);
    frame.Close()?;
    {
        let mut mail = lock(&shared.mailbox);
        if let Err(error) = copied {
            mail.free.push(slot);
            return Err(error);
        }
        if let Some((skipped, _)) = mail.pending.replace((slot, at)) {
            // The encoder hadn't taken the last one yet: the newer picture replaces it.
            mail.free.push(skipped);
            shared.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
    shared.ready.notify_one();
    // Follow a resized window: the pool makes frames at the new size from now on.
    let resized = {
        let mut gpu = lock(&shared.gpu);
        let changed = (size.Width, size.Height) != gpu.content && size.Width > 0 && size.Height > 0;
        if changed {
            gpu.content = (size.Width, size.Height);
        }
        changed.then(|| gpu.device.clone())
    };
    if let Some(device) = resized
        && let Ok(device) = winrt_device(&device)
    {
        let _ = pool.Recreate(&device, DirectXPixelFormat::B8G8R8A8UIntNormalized, 3, size);
    }
    Ok(())
}

/// Copies a captured frame into slot `slot`, black around it when the window is smaller than the recording.
fn copy_frame(
    shared: &Shared,
    frame: &windows::Graphics::Capture::Direct3D11CaptureFrame,
    slot: usize,
    size: windows::Graphics::SizeInt32,
) -> windows::core::Result<()> {
    let source: ID3D11Texture2D =
        // SAFETY: the frame's surface is a Direct3D texture on this device while the frame is open.
        unsafe { frame.Surface()?.cast::<IDirect3DDxgiInterfaceAccess>()?.GetInterface()? };
    let gpu = lock(&shared.gpu);
    let target = &gpu.slots[slot];
    let (width, height) =
        ((size.Width.max(0) as u32).min(gpu.width), (size.Height.max(0) as u32).min(gpu.height));
    // SAFETY: copies inside both textures' bounds on the shared, locked device.
    unsafe {
        if width < gpu.width || height < gpu.height {
            // A window that shrank leaves black around it rather than old pixels.
            let mut view: Option<ID3D11RenderTargetView> = None;
            gpu.device.CreateRenderTargetView(target, None, Some(&mut view))?;
            if let Some(view) = view {
                gpu.context.ClearRenderTargetView(&view, &[0.0, 0.0, 0.0, 1.0]);
            }
        }
        let region = D3D11_BOX { left: 0, top: 0, front: 0, right: width, bottom: height, back: 1 };
        gpu.context.CopySubresourceRegion(target, 0, 0, 0, 0, &source, 0, Some(&region));
    }
    Ok(())
}

fn winrt_device(device: &ID3D11Device) -> windows::core::Result<IDirect3DDevice> {
    let dxgi: IDXGIDevice = device.cast()?;
    // SAFETY: wraps a live DXGI device for WinRT.
    unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }?.cast()
}

/// What the encoding thread works with.
struct Encoding {
    encoder: Encoder,
    /// `None` when the writer converts BGRA itself.
    converter: Option<Converter>,
}

// SAFETY: Media Foundation's sink writer and sample pool are free-threaded, and the converter's Direct3D objects are
// only used under the device's lock.
unsafe impl Send for Encoding {}

/// How the encoding thread ended: the file finished, or why not, and the first frame that couldn't be saved.
struct Encoded {
    finished: Result<(), String>,
    error: Option<String>,
}

impl Encoding {
    /// A sample holding slot `slot`'s picture, or `None` when every NV12 picture is still with the encoder.
    fn picture(&self, shared: &Shared, slot: usize) -> windows::core::Result<Option<IMFSample>> {
        match (&self.converter, &self.encoder.samples) {
            (Some(converter), Some(samples)) => {
                // SAFETY: takes a sample from the pool; it comes back when the encoder releases it.
                let sample = match unsafe { samples.AllocateSample() } {
                    Ok(sample) => sample,
                    Err(error) if error.code() == MF_E_SAMPLEALLOCATOR_EMPTY => return Ok(None),
                    Err(error) => return Err(error),
                };
                let _gpu = lock(&shared.gpu);
                converter.convert(slot, &sample)?;
                Ok(Some(sample))
            }
            _ => {
                let gpu = lock(&shared.gpu);
                let copy = texture(&gpu.device, gpu.width, gpu.height)?;
                // SAFETY: copies one whole texture into another of the same size and format on the locked device.
                unsafe { gpu.context.CopyResource(&copy, &gpu.slots[slot]) };
                drop(gpu);
                sample_of(&copy).map(Some)
            }
        }
    }

    /// Takes frames as they come until the recording ends, then finishes the file.
    fn run(self, shared: &Shared) -> Encoded {
        let mut held: Option<(IMFSample, i64)> = None;
        let mut error: Option<String> = None;
        let mut note = |failure: windows::core::Error| {
            tracing::warn!(%failure, "a frame couldn't be saved");
            error.get_or_insert_with(|| failure.message());
        };
        loop {
            let (job, end) = {
                let mut mail = lock(&shared.mailbox);
                loop {
                    if let Some(job) = mail.pending.take() {
                        break (Some(job), None);
                    }
                    if let Some(end) = mail.end {
                        break (None, Some(end));
                    }
                    mail = shared.ready.wait(mail).unwrap_or_else(PoisonError::into_inner);
                }
            };
            if let Some((slot, at)) = job {
                let picture = self.picture(shared, slot);
                // The slot is free again: the device runs the copy into it, and anything drawn into it next, in order.
                lock(&shared.mailbox).free.push(slot);
                match picture {
                    Ok(Some(sample)) => {
                        if let Some((previous, from)) = held.take()
                            && let Err(failure) = self.encoder.write(&previous, from, at - from)
                        {
                            note(failure);
                        }
                        held = Some((sample, at));
                        shared.frames.fetch_add(1, Ordering::Relaxed);
                    }
                    Ok(None) => {
                        shared.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(failure) => note(failure),
                }
            } else if let Some(end) = end {
                // The last picture stays up until the recording stops.
                if let Some((last, from)) = held.take()
                    && let Err(failure) = self.encoder.write(&last, from, end - from)
                {
                    note(failure);
                }
                let finished = self.encoder.finish().map_err(|e| e.message());
                return Encoded { finished, error };
            }
        }
    }
}

/// The encoding thread. Dropping it without `finish` (a recorder that was never stopped) still ends the thread,
/// which finishes the file.
struct EncodingThread {
    shared: Arc<Shared>,
    handle: Option<JoinHandle<Encoded>>,
}

impl EncodingThread {
    fn start(shared: Arc<Shared>, encoding: Encoding) -> Result<Self, CaptureError> {
        let worker = shared.clone();
        let handle = std::thread::Builder::new()
            .name("screen encoder".into())
            .spawn(move || encoding.run(&worker))
            .map_err(|e| CaptureError::System(format!("the encoder couldn't start: {e}")))?;
        Ok(Self { shared, handle: Some(handle) })
    }

    fn end(&self, at: i64) {
        lock(&self.shared.mailbox).end.get_or_insert(at);
        self.shared.ready.notify_one();
    }

    /// Ends the recording at `at` and waits for the file to be finished.
    fn finish(mut self, at: i64) -> Result<Encoded, CaptureError> {
        self.end(at);
        let handle = self
            .handle
            .take()
            .ok_or_else(|| CaptureError::System("the recording was already finished".into()))?;
        handle.join().map_err(|_| CaptureError::System("the encoder stopped unexpectedly".into()))
    }
}

impl Drop for EncodingThread {
    fn drop(&mut self) {
        if self.handle.is_some() {
            let first = lock(&self.shared.mailbox).first;
            let end = first.map_or(0, |first| ((clock::now() - first).max(0.0) * 1e7).round() as i64);
            self.end(end);
        }
    }
}

pub struct Recorder {
    session: GraphicsCaptureSession,
    pool: Direct3D11CaptureFramePool,
    _item: GraphicsCaptureItem,
    shared: Arc<Shared>,
    encoding: Option<EncodingThread>,
    started: f64,
    output: PathBuf,
    size: (f64, f64),
    scale: f64,
    app_name: Option<String>,
    window_title: Option<String>,
    warnings: Vec<String>,
    input: Option<input::Telemetry>,
    /// The computer's sound, the microphone and the camera, each with the file it goes to.
    system_audio: Option<(AudioRecorder, PathBuf)>,
    microphone: Option<(AudioRecorder, PathBuf)>,
    camera: Option<(CameraRecorder, PathBuf)>,
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
        let slots = (0..SLOTS).map(|_| texture(&device, width, height)).collect::<Result<Vec<_>, _>>();
        let gpu = Gpu {
            device: device.clone(),
            context,
            slots: slots.map_err(system)?,
            width,
            height,
            content: (size.Width, size.Height),
        };

        // The graphics card's encoder keeps up with 4K; Windows' software one is given at most about 1080p, so the
        // recording stays smooth.
        let hardware = hardware_encoder();
        let most = if hardware.is_some() { HARDWARE_PIXELS } else { SOFTWARE_PIXELS };
        let wanted = encode_size(width, height, most);
        let encoding = Converter::new(&gpu, (width, height), wanted, fps)
            .and_then(|converter| {
                let encoder = Encoder::new(&device, &options.output, wanted, fps, true)?;
                Ok(Encoding { encoder, converter: Some(converter) })
            })
            .inspect_err(|error| tracing::warn!(%error, "no NV12 on the graphics card; the encoder converts"))
            .or_else(|_| {
                let encoder = Encoder::new(&device, &options.output, (width, height), fps, false)?;
                Ok::<_, windows::core::Error>(Encoding { encoder, converter: None })
            })
            .map_err(system)?;
        let encoded = if encoding.converter.is_some() { wanted } else { (width, height) };
        tracing::info!(
            captured = ?(width, height),
            encoded = ?encoded,
            fps,
            hardware_encoder = ?hardware,
            converts_on_gpu = encoding.converter.is_some(),
            transforms = ?encoding.encoder.transforms(),
            "screen encoder ready"
        );

        let mut warnings = Vec::new();
        // The sound starts first, so the first words of a take are kept. Without it, the screen still records.
        let mut sound = |source: Source, path: &PathBuf, what: &str| match AudioRecorder::start(source, path)
        {
            Ok(recorder) => Some((recorder, path.clone())),
            Err(error) => {
                warnings.push(format!("{what} isn't recorded: {error}."));
                None
            }
        };
        let microphone = options
            .microphone
            .as_ref()
            .and_then(|m| sound(Source::Microphone(m.device.clone()), &m.output, "The microphone"));
        let system_audio = options
            .system_audio
            .as_ref()
            .and_then(|path| sound(Source::System, path, "The computer's sound"));
        let camera = options.camera.as_ref().and_then(|c| {
            match CameraRecorder::start(c.device.as_deref(), &c.output) {
                Ok(recorder) => Some((recorder, c.output.clone())),
                Err(error) => {
                    warnings.push(format!("The camera isn't recorded: {error}."));
                    None
                }
            }
        });

        let shared = Arc::new(Shared {
            gpu: Mutex::new(gpu),
            mailbox: Mutex::new(Mailbox {
                free: (0..SLOTS).collect(),
                pending: None,
                first: None,
                last: None,
                gap: (1e7 / f64::from(fps) * 0.9) as i64,
                end: None,
            }),
            ready: Condvar::new(),
            frames: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            closed: AtomicBool::new(false),
        });
        let encoding = EncodingThread::start(shared.clone(), encoding)?;
        let direct3d = winrt_device(&device).map_err(system)?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &direct3d,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            3,
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
        tracing::info!(
            system_audio = system_audio.is_some(),
            microphone = microphone.is_some(),
            camera = camera.is_some(),
            ?warnings,
            "screen capture started"
        );
        Ok(Self {
            session,
            pool,
            _item: item,
            shared,
            encoding: Some(encoding),
            started,
            output: options.output,
            size: (f64::from(width) / scale, f64::from(height) / scale),
            scale,
            app_name,
            window_title,
            warnings,
            input,
            system_audio,
            microphone,
            camera,
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
        let first = lock(&self.shared.mailbox).first.unwrap_or(self.started);
        let end = ((stopped - first).max(0.0) * 1e7).round() as i64;
        let encoding = self
            .encoding
            .take()
            .ok_or_else(|| CaptureError::System("the recording was already finished".into()))?;
        let encoded = encoding.finish(end)?;
        if let Some(error) = encoded.error {
            self.warnings.push(format!("Some frames couldn't be saved: {error}"));
        }
        encoded.finished.map_err(CaptureError::System)?;
        let (frames, dropped) =
            (self.shared.frames.load(Ordering::Relaxed), self.shared.dropped.load(Ordering::Relaxed));
        let seconds = end as f64 / 1e7;
        tracing::info!(
            frames,
            dropped,
            seconds,
            fps = if seconds > 0.0 { frames as f64 / seconds } else { 0.0 },
            "screen recording finished"
        );
        if self.shared.closed.load(Ordering::SeqCst) {
            self.warnings.push("The window closed during the recording.".into());
        }
        let (cursor, events) = self.input.take().map(|input| input.stop(first)).unwrap_or_default();

        // Every other track is placed on the screen's timeline: its offset is where it starts relative to the
        // first frame.
        let mut track = |stopped: Option<Ended>, what: &str| {
            let (stopped, output) = stopped?;
            match stopped {
                Ok(Some((start, end))) => Some(RecordedTrack {
                    output,
                    offset_us: ((start - first) * 1e6).round() as i64,
                    duration_us: ((end - start).max(0.0) * 1e6).round() as u64,
                }),
                Ok(None) => None,
                Err(error) => {
                    self.warnings.push(format!("{what} couldn't be saved: {error}."));
                    None
                }
            }
        };
        let system_audio =
            track(self.system_audio.take().map(|(r, path)| (r.stop(), path)), "The computer's sound");
        let microphone = track(self.microphone.take().map(|(r, path)| (r.stop(), path)), "The microphone");
        let camera = track(self.camera.take().map(|(r, path)| (r.stop(), path)), "The camera");
        Ok(Recording {
            output: self.output,
            duration_us: (end / 10).max(0) as u64,
            frames,
            dropped,
            width: self.size.0,
            height: self.size.1,
            scale: self.scale,
            app_name: self.app_name,
            window_title: self.window_title,
            cursor,
            events,
            system_audio,
            microphone,
            camera,
            warnings: self.warnings,
        })
    }
}

// The recorder moves between the app's threads.
const _: () = {
    const fn sendable<T: Send>() {}
    sendable::<Recorder>();
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_captures_keep_their_size() {
        assert_eq!(encode_size(1920, 1080, SOFTWARE_PIXELS), (1920, 1080));
        assert_eq!(encode_size(2560, 1440, HARDWARE_PIXELS), (2560, 1440));
        assert_eq!(encode_size(3840, 2160, HARDWARE_PIXELS), (3840, 2160));
    }

    #[test]
    fn large_captures_are_scaled_to_the_encoder_keeping_their_shape() {
        let (w, h) = encode_size(2880, 1800, SOFTWARE_PIXELS);
        assert!(u64::from(w) * u64::from(h) <= SOFTWARE_PIXELS, "{w}×{h}");
        assert!(w % 2 == 0 && h % 2 == 0, "{w}×{h}");
        assert!((f64::from(w) / f64::from(h) - 1.6).abs() < 0.01, "{w}×{h}");
        // A super-wide display stays within H.264's longest side.
        let (w, h) = encode_size(5120, 1440, HARDWARE_PIXELS);
        assert!(w <= LONGEST_SIDE && w % 2 == 0 && h % 2 == 0, "{w}×{h}");
    }
}
