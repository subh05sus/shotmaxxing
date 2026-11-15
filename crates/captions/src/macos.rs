//! The model's download on macOS, through NSURLSession: the system's own networking, so its proxy settings and
//! certificates apply, and redirects (Hugging Face sends the file from its CDN) are followed.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use block2::RcBlock;
use objc2_foundation::{NSError, NSHTTPURLResponse, NSString, NSURL, NSURLResponse, NSURLSession};

use crate::CaptionError;

/// Downloads `url` into the file `to`. `progress` hears the bytes received and expected (`expected` until the
/// server says), about ten times a second, and cancels by returning false.
pub fn download(
    url: &str,
    to: &Path,
    expected: u64,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<(), CaptionError> {
    let url = NSURL::URLWithString(&NSString::from_str(url))
        .ok_or_else(|| CaptionError::Download("the address isn't valid".into()))?;
    let (done, finished) = channel::<Result<(), String>>();
    let target = to.to_path_buf();
    let handler =
        RcBlock::new(move |location: *mut NSURL, response: *mut NSURLResponse, error: *mut NSError| {
            // SAFETY: the session passes valid objects or null, alive for this call; the file at `location` exists
            // until the handler returns.
            let result = unsafe { keep(location.as_ref(), response.as_ref(), error.as_ref(), &target) };
            let _ = done.send(result);
        });
    let session = NSURLSession::sharedSession();
    // SAFETY: the handler moves a file and sends on a channel, both fine on the session's own queue.
    let task = unsafe { session.downloadTaskWithURL_completionHandler(&url, &handler) };
    task.resume();
    loop {
        match finished.recv_timeout(Duration::from_millis(100)) {
            Ok(result) => {
                let got = task.countOfBytesReceived().max(0) as u64;
                progress(got, got.max(expected));
                return result.map_err(CaptionError::Download);
            }
            Err(RecvTimeoutError::Timeout) => {
                let got = task.countOfBytesReceived().max(0) as u64;
                let total = u64::try_from(task.countOfBytesExpectedToReceive()).ok().filter(|&t| t > 0);
                if !progress(got, total.unwrap_or(expected)) {
                    task.cancel();
                    // The handler still runs, with a cancellation; wait so nothing is left behind.
                    let _ = finished.recv_timeout(Duration::from_secs(5));
                    let _ = std::fs::remove_file(to);
                    return Err(CaptionError::Cancelled);
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(CaptionError::Download("the download stopped".into()));
            }
        }
    }
}

/// Moves the finished download to `to` (the system deletes it once the handler returns), or says what went wrong.
///
/// # Safety
/// Called from the completion handler, while the file at `location` still exists.
unsafe fn keep(
    location: Option<&NSURL>,
    response: Option<&NSURLResponse>,
    error: Option<&NSError>,
    to: &Path,
) -> Result<(), String> {
    if let Some(error) = error {
        return Err(error.localizedDescription().to_string());
    }
    if let Some(http) = response.and_then(|r| r.downcast_ref::<NSHTTPURLResponse>()) {
        let status = http.statusCode();
        if status != 200 {
            return Err(format!("the server answered {status}"));
        }
    }
    let from = location.and_then(|l| l.path()).ok_or("the download left no file")?;
    let from = PathBuf::from(from.to_string());
    std::fs::rename(&from, to).or_else(|_| std::fs::copy(&from, to).map(drop)).map_err(|e| e.to_string())
}
