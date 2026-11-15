//! The model's download on Windows, through WinHTTP: the system's own networking, so its proxy settings and
//! certificates apply, and redirects (Hugging Face sends the file from its CDN) are followed.

use std::ffi::c_void;
use std::io::Write;
use std::path::Path;

use windows::Win32::Networking::WinHttp::{
    INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_QUERY_CONTENT_LENGTH, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_FLAG_NUMBER64,
    WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
};
use windows::core::{HSTRING, PCWSTR, w};

use crate::CaptionError;

/// A WinHTTP handle, closed when dropped.
struct Handle(*mut c_void);

impl Handle {
    /// The handle, or what Windows said when it couldn't make it.
    fn new(handle: *mut c_void, what: &str) -> Result<Self, CaptionError> {
        if handle.is_null() {
            Err(CaptionError::Download(format!("{what}: {}", windows::core::Error::from_thread().message())))
        } else {
            Ok(Self(handle))
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: a handle WinHTTP gave this, closed once.
        let _ = unsafe { WinHttpCloseHandle(self.0) };
    }
}

fn failed(what: &'static str) -> impl Fn(windows::core::Error) -> CaptionError {
    move |e| CaptionError::Download(format!("{what}: {}", e.message()))
}

/// Downloads `url` (https) into the file `to`. `progress` hears the bytes received and expected (`expected` until
/// the server says), and cancels by returning false.
pub fn download(
    url: &str,
    to: &Path,
    expected: u64,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<(), CaptionError> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| CaptionError::Download("the address isn't https".into()))?;
    let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let path = if path.is_empty() { "/" } else { path };
    // SAFETY: WinHTTP calls on handles this function owns, with buffers that outlive each call.
    unsafe {
        let session = Handle::new(
            WinHttpOpen(
                w!("Shotmaxxing"),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            ),
            "couldn't start a connection",
        )?;
        // Resolving, connecting and sending each get 30 s; receiving, a minute between packets.
        WinHttpSetTimeouts(session.0, 30_000, 30_000, 30_000, 60_000)
            .map_err(failed("couldn't set timeouts"))?;
        let connect = Handle::new(
            WinHttpConnect(session.0, &HSTRING::from(host), INTERNET_DEFAULT_HTTPS_PORT, 0),
            "couldn't reach the server",
        )?;
        let request = Handle::new(
            WinHttpOpenRequest(
                connect.0,
                w!("GET"),
                &HSTRING::from(path),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            ),
            "couldn't make the request",
        )?;
        WinHttpSendRequest(request.0, None, None, 0, 0, 0).map_err(failed("couldn't send the request"))?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut()).map_err(failed("no answer"))?;
        let mut status = 0u32;
        let mut size = size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&raw mut status).cast()),
            &mut size,
            std::ptr::null_mut(),
        )
        .map_err(failed("no status"))?;
        if status != 200 {
            return Err(CaptionError::Download(format!("the server answered {status}")));
        }
        let mut length = 0u64;
        let mut size = size_of::<u64>() as u32;
        let total = match WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_CONTENT_LENGTH | WINHTTP_QUERY_FLAG_NUMBER64,
            PCWSTR::null(),
            Some((&raw mut length).cast()),
            &mut size,
            std::ptr::null_mut(),
        ) {
            Ok(()) if length > 0 => length,
            _ => expected,
        };
        let mut file = std::io::BufWriter::new(std::fs::File::create(to)?);
        let mut buffer = vec![0u8; 256 * 1024];
        let mut got = 0u64;
        loop {
            if !progress(got, total) {
                drop(file);
                let _ = std::fs::remove_file(to);
                return Err(CaptionError::Cancelled);
            }
            let mut read = 0u32;
            WinHttpReadData(request.0, buffer.as_mut_ptr().cast(), buffer.len() as u32, &mut read)
                .map_err(failed("the download stopped"))?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read as usize])?;
            got += u64::from(read);
        }
        file.flush()?;
        progress(got, total.max(got));
        Ok(())
    }
}
