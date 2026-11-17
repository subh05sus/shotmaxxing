//! What the app notes as it runs, kept in a file someone can send when something goes wrong: on Windows in
//! `%LOCALAPPDATA%\Shotmaxxing\logs`, on macOS in `~/Library/Logs/Shotmaxxing`. Every launch adds to
//! `shotmaxxing.log`, so the run that crashed is still there after the app is opened again; past a few megabytes it
//! becomes `shotmaxxing.previous.log` and a new one starts. A panic is written down with where it happened, and on
//! Windows so is a crash in native code, with the module it happened in.

use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::Mutex;

use tracing_subscriber::fmt::writer::MakeWriterExt;

const FILE: &str = "shotmaxxing.log";
const PREVIOUS: &str = "shotmaxxing.previous.log";
/// How big the log grows before a new one starts.
const MOST: u64 = 5 * 1024 * 1024;

/// The folder the log files are in.
pub fn folder() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("Shotmaxxing"));
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Logs/Shotmaxxing"));
    #[cfg(not(any(windows, target_os = "macos")))]
    let base = std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state/shotmaxxing"));
    #[cfg(target_os = "macos")]
    return base;
    #[cfg(not(target_os = "macos"))]
    base.map(|dir| dir.join("logs"))
}

/// The log file, once there is one.
pub fn file() -> Option<PathBuf> {
    folder().map(|dir| dir.join(FILE)).filter(|file| file.exists())
}

/// Starts logging to the terminal and to the file, and writes down panics and crashes. Logging still goes to the
/// terminal when there's no folder to write in.
pub fn start() {
    let filter = || {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
    };
    let file = folder().and_then(|dir| {
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join(FILE);
        if std::fs::metadata(&path).is_ok_and(|file| file.len() > MOST) {
            let _ = std::fs::rename(&path, dir.join(PREVIOUS));
        }
        // Added to, not replaced: a second launch, which only hands over to the running app, mustn't wipe its log.
        OpenOptions::new().create(true).append(true).open(&path).ok()
    });
    // Each line is written straight to the file, unbuffered, so it's there even if the app ends suddenly.
    match file {
        Some(file) => tracing_subscriber::fmt()
            .with_env_filter(filter())
            .with_ansi(false)
            .with_writer(std::io::stderr.and(Mutex::new(file)))
            .init(),
        None => tracing_subscriber::fmt().with_env_filter(filter()).init(),
    }
    tracing::info!(version = env!("CARGO_PKG_VERSION"), os = std::env::consts::OS, "Shotmaxxing started");

    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let backtrace = std::backtrace::Backtrace::force_capture();
        tracing::error!(thread = thread.name().unwrap_or("unnamed"), "panic: {info}\n{backtrace}");
        before(info);
    }));
    #[cfg(windows)]
    crashes::watch();
}

/// Native crashes on Windows: an access violation in a driver, say. Rust's own panics are caught above; these are
/// what Windows would otherwise only put in its event log.
#[cfg(windows)]
mod crashes {
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::System::Diagnostics::Debug::{EXCEPTION_POINTERS, SetUnhandledExceptionFilter};
    use windows::Win32::System::LibraryLoader::{
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        GetModuleFileNameW, GetModuleHandleExW,
    };
    use windows::core::PCWSTR;

    /// Lets Windows go on to its own crash handling, after this one has written the crash down.
    const CONTINUE_SEARCH: i32 = 0;

    pub fn watch() {
        // SAFETY: installs a process-wide filter; it's a plain function that lives as long as the process.
        unsafe {
            SetUnhandledExceptionFilter(Some(crashed));
        }
    }

    unsafe extern "system" fn crashed(info: *const EXCEPTION_POINTERS) -> i32 {
        // SAFETY: Windows passes the crash's records, valid for this call; both pointers are checked.
        let (code, address) = unsafe {
            match info.as_ref().and_then(|info| info.ExceptionRecord.as_ref()) {
                Some(record) => (record.ExceptionCode.0 as u32, record.ExceptionAddress),
                None => return CONTINUE_SEARCH,
            }
        };
        let thread = std::thread::current();
        tracing::error!(
            thread = thread.name().unwrap_or("unnamed"),
            "crash: exception {code:#010x} at {address:?} in {}",
            module(address).unwrap_or_else(|| "an unknown module".into())
        );
        CONTINUE_SEARCH
    }

    /// The file of the module that holds `address`.
    fn module(address: *const std::ffi::c_void) -> Option<String> {
        let mut handle = HMODULE::default();
        let mut name = [0u16; 512];
        // SAFETY: looks the address up without changing the module's reference count, then reads its file name
        // into a buffer of the size given.
        unsafe {
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                PCWSTR(address.cast()),
                &mut handle,
            )
            .ok()?;
            let length = GetModuleFileNameW(Some(handle), &mut name) as usize;
            (length > 0).then(|| String::from_utf16_lossy(&name[..length]))
        }
    }
}
