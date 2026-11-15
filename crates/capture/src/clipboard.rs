//! Pictures onto the clipboard: PNG for every app that reads it, and on Windows a bitmap too, for the apps that only
//! read those.

use crate::CaptureError;

/// Puts a picture on the clipboard in place of what was there. `png` is the picture; `rgba` holds the same pixels
/// (straight RGBA, top row first) for systems that also want a bitmap.
pub fn copy_image(png: &[u8], rgba: &[u8], width: u32, height: u32) -> Result<(), CaptureError> {
    if rgba.len() != width as usize * height as usize * 4 {
        return Err(CaptureError::System("the picture's pixels don't match its size".into()));
    }
    imp::copy_image(png, rgba, width, height)
}

/// A bottom-up 32-bit device-independent bitmap (a `BITMAPINFOHEADER`, then the rows) of straight RGBA pixels.
#[cfg_attr(not(windows), allow(dead_code))]
fn dib(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let row = width as usize * 4;
    let mut out = Vec::with_capacity(40 + rgba.len());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(width as i32).to_le_bytes());
    // Positive: the bottom row first.
    out.extend_from_slice(&(height as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    // BI_RGB, then the size of the pixels, and no resolution or palette.
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&((row * height as usize) as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    for line in rgba.chunks_exact(row).rev() {
        for pixel in line.as_chunks::<4>().0 {
            out.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    out
}

#[cfg(target_os = "macos")]
mod imp {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypePNG};
    use objc2_foundation::NSData;

    use crate::CaptureError;

    pub fn copy_image(png: &[u8], _rgba: &[u8], _width: u32, _height: u32) -> Result<(), CaptureError> {
        let data = NSData::with_bytes(png);
        let board = NSPasteboard::generalPasteboard();
        board.clearContents();
        // SAFETY: AppKit's constant name for PNG data.
        let kind = unsafe { NSPasteboardTypePNG };
        if board.setData_forType(Some(&data), kind) {
            Ok(())
        } else {
            Err(CaptureError::System("macOS didn't take the picture onto the clipboard".into()))
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::time::Duration;

    use windows::Win32::Foundation::{GlobalFree, HANDLE};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
    use windows::Win32::System::Ole::CF_DIB;
    use windows::core::w;

    use crate::CaptureError;

    pub fn copy_image(png: &[u8], rgba: &[u8], width: u32, height: u32) -> Result<(), CaptureError> {
        let bitmap = super::dib(rgba, width, height);
        // Another app may hold the clipboard open for a moment.
        let mut open = false;
        for _ in 0..10 {
            // SAFETY: opens the clipboard for this thread; it's closed again below.
            if unsafe { OpenClipboard(None) }.is_ok() {
                open = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if !open {
            return Err(CaptureError::System("another app is using the clipboard. Try again.".into()));
        }
        // SAFETY: the clipboard is open on this thread until CloseClipboard.
        let copied = unsafe {
            EmptyClipboard().and_then(|()| {
                let format = RegisterClipboardFormatW(w!("PNG"));
                if format != 0 {
                    put(format, png)?;
                }
                put(u32::from(CF_DIB.0), &bitmap)
            })
        };
        // SAFETY: closes the clipboard this thread opened.
        let _ = unsafe { CloseClipboard() };
        copied.map_err(|e| CaptureError::System(format!("couldn't copy the picture: {}", e.message())))
    }

    /// Hands `bytes` to the open clipboard as `format`. Once the clipboard takes them it owns the memory.
    ///
    /// # Safety
    /// The clipboard must be open on this thread.
    unsafe fn put(format: u32, bytes: &[u8]) -> windows::core::Result<()> {
        // SAFETY: the memory is allocated, filled within its size and unlocked here, and freed unless the
        // clipboard took it.
        unsafe {
            let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len())?;
            let target = GlobalLock(memory);
            if target.is_null() {
                let _ = GlobalFree(Some(memory));
                return Err(windows::core::Error::from_thread());
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), target.cast::<u8>(), bytes.len());
            // Reports an "error" once the memory is no longer locked, which is what's wanted.
            let _ = GlobalUnlock(memory);
            if let Err(error) = SetClipboardData(format, Some(HANDLE(memory.0))) {
                let _ = GlobalFree(Some(memory));
                return Err(error);
            }
            Ok(())
        }
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod imp {
    use crate::CaptureError;

    pub fn copy_image(_png: &[u8], _rgba: &[u8], _width: u32, _height: u32) -> Result<(), CaptureError> {
        Err(CaptureError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::dib;

    #[test]
    fn bitmaps_start_at_the_bottom_row_in_blue_green_red() {
        // Two pixels wide, two tall: red and green on top, blue and white below.
        let rgba = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
        let bitmap = dib(&rgba, 2, 2);
        assert_eq!(bitmap.len(), 40 + 16);
        assert_eq!(&bitmap[4..12], &[2, 0, 0, 0, 2, 0, 0, 0]);
        assert_eq!(&bitmap[40..48], &[255, 0, 0, 255, 255, 255, 255, 255], "blue, then white");
        assert_eq!(&bitmap[48..56], &[0, 0, 255, 255, 0, 255, 0, 255], "red, then green");
    }
}
