//! Pictures of the displays and windows: small ones for the source picker, and full-size screenshots. GDI copies
//! them: a display straight off the screen, a window through PrintWindow, which takes its full content from the
//! compositor, so whatever covers it doesn't show and no capture border flashes. Thumbnails are shrunk with halftone
//! filtering and saved as JPEG with WIC; screenshots are copied pixel for pixel and saved as PNG.

use std::ffi::c_void;

use windows::Win32::Foundation::{HGLOBAL, HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CAPTUREBLT, COLORONCOLOR, CreateCompatibleBitmap,
    CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, HALFTONE, HBITMAP, HDC,
    ROP_CODE, ReleaseDC, SRCCOPY, SelectObject, SetBrushOrgEx, SetStretchBltMode, StretchBlt,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatJpeg, GUID_ContainerFormatPng, GUID_WICPixelFormat24bppBGR,
    IWICImagingFactory, WICBitmapEncoderNoCache,
};
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, STATFLAG_NONAME, STATSTG, STREAM_SEEK_SET,
};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, IsHungAppWindow, IsIconic, PW_RENDERFULLCONTENT,
};
use windows::core::{GUID, IUnknown};

use super::record::start_systems;
use super::{dpi_aware, monitor, window_bounds};
use crate::{CaptureError, Screenshot, Target};

/// A picture: BGRA, top row first.
struct Picture {
    width: i32,
    height: i32,
    pixels: Vec<u8>,
}

/// WIC's factory, which any thread of this process may use.
fn imaging() -> Result<IWICImagingFactory, CaptureError> {
    dpi_aware();
    start_systems()?;
    // SAFETY: creates a COM object on a thread start_systems set up for COM.
    unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None::<&IUnknown>, CLSCTX_INPROC_SERVER) }
        .map_err(|e| CaptureError::System(e.message()))
}

pub fn thumbnails(targets: &[Target], max_width: u32) -> Result<Vec<Option<crate::Thumbnail>>, CaptureError> {
    let factory = imaging()?;
    let max_width = max_width.clamp(16, 4096) as i32;
    Ok(targets
        .iter()
        .map(|&target| {
            let picture = match target {
                Target::Display(number) => display(monitor(number).ok()?.bounds, max_width),
                Target::Window(id) => window(HWND(id as usize as *mut c_void), max_width),
            }?;
            encode(&factory, &picture, &GUID_ContainerFormatJpeg).map(crate::Thumbnail::Picture)
        })
        .collect())
}

/// A full-size PNG of `target`, without the pointer: a screen as it shows, everything on it included; a window alone.
pub fn screenshot(target: Target) -> Result<Screenshot, CaptureError> {
    let factory = imaging()?;
    let (picture, scale) = match target {
        Target::Display(number) => {
            let monitor = monitor(number)?;
            (display(monitor.bounds, i32::MAX), monitor.scale)
        }
        Target::Window(id) => {
            let hwnd = HWND(id as usize as *mut c_void);
            // SAFETY: a query on a window handle; one that's gone answers 0.
            let dpi = unsafe { GetDpiForWindow(hwnd) };
            (window(hwnd, i32::MAX), f64::from(dpi.max(48)) / 96.0)
        }
    };
    let picture = picture.ok_or_else(|| {
        CaptureError::System(
            "Windows couldn't copy that picture. A minimised window has nothing to show.".into(),
        )
    })?;
    let png = encode(&factory, &picture, &GUID_ContainerFormatPng)
        .ok_or_else(|| CaptureError::System("couldn't save the picture as PNG".into()))?;
    Ok(Screenshot {
        png,
        width: picture.width as u32,
        height: picture.height as u32,
        scale,
        app_name: None,
        window_title: None,
    })
}

/// `width` × `height` shrunk to at most `max_width` wide, never enlarged.
fn fit(width: i32, height: i32, max_width: i32) -> (i32, i32) {
    let w = width.min(max_width).max(1);
    let h = (i64::from(height) * i64::from(w) + i64::from(width) / 2) / i64::from(width.max(1));
    (w, (h as i32).max(1))
}

fn display(bounds: RECT, max_width: i32) -> Option<Picture> {
    let (width, height) = (bounds.right - bounds.left, bounds.bottom - bounds.top);
    if width <= 0 || height <= 0 {
        return None;
    }
    // SAFETY: the screen's device context is released after the copy.
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }
        let picture =
            shrink(screen, screen, (bounds.left, bounds.top, width, height), fit(width, height, max_width));
        ReleaseDC(None, screen);
        picture
    }
}

fn window(window: HWND, max_width: i32) -> Option<Picture> {
    // SAFETY: queries on a window handle, and a bitmap and device contexts that are freed before returning.
    unsafe {
        // A window that stopped responding could hold the copy up, and a minimised one has nothing to show.
        if IsHungAppWindow(window).as_bool() || IsIconic(window).as_bool() {
            return None;
        }
        let mut whole = RECT::default();
        GetWindowRect(window, &mut whole).ok()?;
        // The frame you see, without the invisible borders Windows keeps around it for resizing.
        let frame = window_bounds(window).unwrap_or(whole);
        let (width, height) = (whole.right - whole.left, whole.bottom - whole.top);
        let (seen_width, seen_height) = (frame.right - frame.left, frame.bottom - frame.top);
        if width <= 0 || height <= 0 || seen_width <= 0 || seen_height <= 0 {
            return None;
        }
        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let previous = SelectObject(memory, bitmap.into());
        let printed = PrintWindow(window, memory, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)).as_bool();
        let area = (frame.left - whole.left, frame.top - whole.top, seen_width, seen_height);
        let picture = if printed {
            shrink(screen, memory, area, fit(seen_width, seen_height, max_width))
        } else {
            None
        };
        SelectObject(memory, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
        picture
    }
}

/// `area` (left, top, width, height) of `source` shrunk to `size` with halftone filtering. `screen` is the screen's
/// device context, which the new bitmap matches.
///
/// # Safety
/// `screen` and `source` must be valid device contexts.
unsafe fn shrink(screen: HDC, source: HDC, area: (i32, i32, i32, i32), size: (i32, i32)) -> Option<Picture> {
    // SAFETY: the caller's device contexts are valid; the bitmap and context made here are freed here.
    unsafe {
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, size.0, size.1);
        let previous = SelectObject(memory, bitmap.into());
        // Halftone averages pixels even when nothing shrinks; a copy at full size stays exact.
        let exact = (size.0, size.1) == (area.2, area.3);
        SetStretchBltMode(memory, if exact { COLORONCOLOR } else { HALFTONE });
        // Halftone filtering wants the brush origin set after the mode.
        let _ = SetBrushOrgEx(memory, 0, 0, None);
        let copied = StretchBlt(
            memory,
            0,
            0,
            size.0,
            size.1,
            Some(source),
            area.0,
            area.1,
            area.2,
            area.3,
            ROP_CODE(SRCCOPY.0 | CAPTUREBLT.0),
        )
        .as_bool();
        // The bitmap is read once it's no longer selected, as GetDIBits asks.
        SelectObject(memory, previous);
        let picture = if copied { read(memory, bitmap, size) } else { None };
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        picture
    }
}

/// A bitmap's pixels, as BGRA from the top row down.
///
/// # Safety
/// `context` must be a valid device context and `bitmap` a bitmap of `size` not selected into any.
unsafe fn read(context: HDC, bitmap: HBITMAP, (width, height): (i32, i32)) -> Option<Picture> {
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Negative: the top row first.
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    // SAFETY: `pixels` holds exactly the rows asked for, at 32 bits a pixel.
    let rows = unsafe {
        GetDIBits(
            context,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        )
    };
    (rows == height).then_some(Picture { width, height, pixels })
}

/// The picture saved in `container`'s format (JPEG or PNG), without alpha.
fn encode(factory: &IWICImagingFactory, picture: &Picture, container: &GUID) -> Option<Vec<u8>> {
    let (width, height) = (picture.width as u32, picture.height as u32);
    let bgr: Vec<u8> = picture.pixels.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    // SAFETY: WIC calls with valid objects. The pixels outlive the call that reads them, and the stream is read
    // within the size it reports.
    unsafe {
        let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true).ok()?;
        let encoder = factory.CreateEncoder(container, std::ptr::null()).ok()?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache).ok()?;
        let (mut frame, mut options) = (None, None);
        encoder.CreateNewFrame(&mut frame, &mut options).ok()?;
        let frame = frame?;
        frame.Initialize(options.as_ref()).ok()?;
        frame.SetSize(width, height).ok()?;
        let mut format = GUID_WICPixelFormat24bppBGR;
        frame.SetPixelFormat(&mut format).ok()?;
        if format != GUID_WICPixelFormat24bppBGR {
            return None;
        }
        frame.WritePixels(height, width * 3, &bgr).ok()?;
        frame.Commit().ok()?;
        encoder.Commit().ok()?;
        let mut stat = STATSTG::default();
        stream.Stat(&mut stat, STATFLAG_NONAME).ok()?;
        stream.Seek(0, STREAM_SEEK_SET, None).ok()?;
        let mut bytes = vec![0u8; usize::try_from(stat.cbSize).ok()?];
        let mut read = 0u32;
        stream.Read(bytes.as_mut_ptr().cast(), bytes.len() as u32, Some(&mut read)).ok().ok()?;
        bytes.truncate(read as usize);
        Some(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_jpegs_of_the_main_display_and_some_windows() {
        let mut targets = vec![Target::Display(None)];
        if let Ok(sources) = super::super::sources() {
            targets.extend(sources.windows.iter().take(3).map(|w| Target::Window(w.id)));
        }
        let pictures = thumbnails(&targets, 320).unwrap();
        assert_eq!(pictures.len(), targets.len());
        for (target, picture) in targets.iter().zip(&pictures) {
            match picture {
                Some(crate::Thumbnail::Picture(bytes)) => {
                    assert_eq!(bytes[..3], [0xFF, 0xD8, 0xFF], "{target:?} isn't a JPEG");
                    eprintln!("{target:?}: a {} byte JPEG", bytes.len());
                }
                Some(crate::Thumbnail::Parked { .. } | crate::Thumbnail::Away { .. }) => {
                    panic!("{target:?}: parked or away, but Windows has no Stage Manager or Spaces")
                }
                None => eprintln!("{target:?}: no picture"),
            }
        }
    }

    #[test]
    fn sizes_shrink_to_fit_and_never_grow() {
        assert_eq!(fit(3840, 2160, 480), (480, 270));
        assert_eq!(fit(1000, 333, 480), (480, 160));
        assert_eq!(fit(200, 100, 480), (200, 100));
        assert_eq!(fit(5000, 1, 480), (480, 1));
    }
}
