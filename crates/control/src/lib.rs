//! The control API: how the `shotmaxxing` command, and through it every connected agent, reaches the running app.
//!
//! The app listens where only this user can reach it: a Unix socket in its own support folder on macOS and Linux, and
//! a local named pipe on Windows. For each thing it needs, the command connects, says hello and sends one request;
//! messages are JSON, one per line.
//!
//! The app does everything that takes the system's permissions: it lists what can be recorded, looks at an app before
//! a take, records, and films takes, all with its own permissions, so an agent's terminal needs none. Before it uses
//! them for an agent it asks the person: once per agent session before it lets the agent see or record the screen,
//! and before every take. The person sees who is asking as the system reports it ([`Caller`]), not as the request
//! claims.

// Naming the caller and the Windows pipe need system calls the standard library doesn't wrap. Each module that makes
// them says so.
#![deny(unsafe_code)]

use std::io::{self, BufRead, BufReader, Read, Write};
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
pub use shotmaxxing_core::APP_ID;
use shotmaxxing_core::refs::ControlSnapshot;
use shotmaxxing_core::take::TakeScript;

pub mod caller;
pub use caller::Caller;

/// The protocol this build speaks. The command sends it in [`ToApp::Hello`] and the app answers with its own in
/// [`FromApp::Welcome`], so a command and an app from different versions say so instead of guessing.
pub const PROTOCOL: u32 = 2;

/// The longest message accepted, so a confused peer can't fill memory. A window's picture travels in one, as base64.
const MAX_LINE: u64 = 8 << 20;

/// How long the command waits for the app to answer its hello.
const HELLO_SECONDS: u64 = 5;

/// A take an agent wants to film.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TakeRequest {
    /// The script, already checked by the command.
    pub script: TakeScript,
    /// The controls the agent saw when it prepared the take, to check refs against.
    #[serde(default)]
    pub planned: Vec<ControlSnapshot>,
    /// Each beat in words, as the person sees them before saying yes.
    pub beats: Vec<String>,
    /// About how long the take runs, in seconds.
    pub seconds: f64,
    /// Where the project goes.
    pub folder: PathBuf,
    /// A microphone id, `default` for the system's choice, or nothing for no microphone.
    #[serde(default)]
    pub microphone: Option<String>,
    /// Record what the computer plays.
    #[serde(default)]
    pub system_audio: bool,
    /// The plan in plain steps, as the agent showed it to the person, for the take card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<TakePlan>,
}

/// What a take is for, in the agent's words: a title and plain steps, the way it showed the person before filming.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TakePlan {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub steps: Vec<String>,
}

/// How a take went, once the app has saved it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Performed {
    /// The saved project.
    pub project: PathBuf,
    pub beats: usize,
    pub beats_done: usize,
    /// Why the take stopped early, in a sentence; `None` when every beat played.
    pub stopped: Option<String>,
    /// Captions added from `say` beats.
    pub captions: usize,
    /// How many times the person had control (cut from the video).
    pub handed_over: usize,
    /// What the agent should know: a microphone that wasn't allowed, dropped frames, a ref that moved.
    pub warnings: Vec<String>,
    /// The take's length in seconds.
    pub seconds: f64,
}

/// A picture of a window for an agent: a JPEG in base64, and its size in pixels.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Picture {
    pub data: String,
    pub width: u32,
    pub height: u32,
}

impl Picture {
    pub fn from_jpeg(jpeg: &[u8]) -> Self {
        let (width, height) = jpeg_size(jpeg).unwrap_or_default();
        Self { data: base64(jpeg), width, height }
    }
}

/// A JPEG's size in pixels, from its frame header.
fn jpeg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2;
    while i + 9 < bytes.len() {
        if bytes[i] != 0xff {
            return None;
        }
        let marker = bytes[i + 1];
        let length = usize::from(u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]));
        if (0xc0..=0xcf).contains(&marker) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
            let height = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]);
            let width = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]);
            return Some((u32::from(width), u32::from(height)));
        }
        i += 2 + length;
    }
    None
}

/// Standard base64, with padding.
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(TABLE[((n >> shift) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// How a take ended, as the take card shows it once the command's own work (an export) is done too.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TakeResult {
    pub beats_done: usize,
    /// Why it stopped early, in a sentence; `None` when every beat played.
    pub stopped: Option<String>,
    /// The saved project.
    pub project: Option<PathBuf>,
    /// The exported video, when the script asked for one.
    pub video: Option<PathBuf>,
    /// The video's length in seconds.
    pub seconds: f64,
}

/// A recording an agent wants to start: a window by id, else the biggest window of an app, else a display.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RecordRequest {
    pub window: Option<u32>,
    pub app: Option<String>,
    /// A display id; the main display when nothing is chosen.
    pub display: Option<u32>,
    pub name: String,
    pub folder: PathBuf,
    /// The most frames per second.
    pub fps: u32,
    pub hide_desktop_icons: bool,
    pub system_audio: bool,
    /// A microphone id, `default`, or nothing.
    pub microphone: Option<String>,
    /// A camera id, `default`, or nothing.
    pub camera: Option<String>,
    /// It stops by itself (and is kept) after this long.
    pub max_seconds: f64,
}

/// A recording an agent started, while it runs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RecordingStatus {
    pub project: PathBuf,
    pub seconds: f64,
    pub stops_after_seconds: f64,
}

/// A recording that ended.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Recorded {
    /// The saved project; `None` when it was discarded.
    pub project: Option<PathBuf>,
    pub dropped_frames: u64,
    pub warnings: Vec<String>,
}

/// From the command to the app. Every connection starts with [`ToApp::Hello`] and carries one request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToApp {
    /// Who the command works for, as the agent named itself, and its session (one `shotmaxxing mcp` process). The
    /// app answers with [`FromApp::Welcome`].
    Hello { protocol: u32, client: String, session: String },
    /// Lists the displays and windows that can be recorded: [`FromApp::Sources`].
    Sources,
    /// Reads an app's controls, and takes a picture of its window when `picture` is set, to plan a take:
    /// [`FromApp::Prepared`].
    Prepare { app: Option<String>, window: Option<String>, picture: bool },
    /// Films a take once the person allows it: [`FromApp::Answer`], [`FromApp::Progress`] while it plays, then
    /// [`FromApp::Performed`].
    Take { take: Box<TakeRequest> },
    /// The command finished its own work on the take (an export): the card shows how it ended.
    Finished { result: TakeResult },
    /// The agent called the take off. What was filmed so far is kept.
    Cancel,
    /// Starts recording for the agent: [`FromApp::Recording`].
    Record { record: RecordRequest },
    /// Asks after the recording this session started: [`FromApp::Recording`].
    RecordingStatus,
    /// Stops this session's recording: [`FromApp::Recorded`].
    StopRecording { discard: bool },
    /// Opens a saved project (its folder) in the editor: [`FromApp::Opened`]. Nothing is filmed or driven, so the
    /// app doesn't ask the person.
    Open { project: PathBuf },
}

/// From the app to the command.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FromApp {
    /// The answer to [`ToApp::Hello`]: the app's protocol and version.
    Welcome { protocol: u32, version: String },
    /// The person's answer before a take, with the reason when it's no.
    Answer {
        allowed: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// What can be recorded: `shotmaxxing_capture::Sources` as JSON.
    Sources { sources: Value },
    /// What preparing found (`shotmaxxing_agent::Prepared` as JSON), with a picture of the window.
    Prepared {
        prepared: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        picture: Option<Picture>,
    },
    /// A take moving on: `done` from 0 to 1, and what's happening in words.
    Progress { done: f64, message: String },
    /// How the take went.
    Performed { performed: Performed },
    /// The session's recording, or `None` when it isn't recording.
    Recording { status: Option<RecordingStatus> },
    /// The session's recording ended.
    Recorded { recorded: Recorded },
    /// The project is open in the editor.
    Opened,
    /// The request couldn't be done, or the person said no: why, in a sentence.
    Failed { message: String },
}

/// Where the app listens: `~/Library/Application Support/<APP_ID>/control.sock` on macOS, the runtime folder (or the
/// data folder) on Linux, and the pipe `\\.\pipe\<APP_ID>.control.<this user's SID>` on Windows. `None` without a
/// home folder, or when the path is too long for a socket. `SHOTMAXXING_CONTROL_SOCKET` points both ends somewhere
/// else, for tests and development (on Windows, another pipe name).
pub fn socket_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SHOTMAXXING_CONTROL_SOCKET").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(path));
    }
    default_path()
}

#[cfg(unix)]
fn default_path() -> Option<PathBuf> {
    let path = if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support").join(APP_ID)
    } else {
        match std::env::var_os("XDG_RUNTIME_DIR") {
            Some(runtime) => PathBuf::from(runtime).join(APP_ID),
            None => PathBuf::from(std::env::var_os("HOME")?).join(".local/share").join(APP_ID),
        }
    }
    .join("control.sock");
    // Socket paths are limited to 104 bytes on macOS, including the final zero.
    (path.as_os_str().len() < 104).then_some(path)
}

#[cfg(windows)]
fn default_path() -> Option<PathBuf> {
    pipe::name()
}

#[cfg(not(any(unix, windows)))]
fn default_path() -> Option<PathBuf> {
    None
}

/// A private address for tests: a socket in `dir` on Unix, and on Windows a pipe no other test or app uses.
#[doc(hidden)]
pub fn scratch_address(dir: &std::path::Path) -> PathBuf {
    if cfg!(windows) {
        use std::sync::atomic::AtomicU32;
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        PathBuf::from(format!(r"\\.\pipe\{APP_ID}.test.{}.{n}.{}", std::process::id(), nanos.subsec_nanos()))
    } else {
        dir.join("control.sock")
    }
}

/// Writes one message as a line of JSON.
pub fn write_message<T: Serialize>(out: &mut impl Write, message: &T) -> io::Result<()> {
    let mut line = serde_json::to_vec(message)?;
    line.push(b'\n');
    out.write_all(&line)?;
    out.flush()
}

/// Reads lines into `line` until one holds a whole message, skipping blank lines and kinds this version doesn't
/// know. Partial lines stay in `line`, so a read that times out can simply be tried again. `None` at the end.
fn read_into<T: DeserializeOwned>(reader: &mut impl BufRead, line: &mut String) -> io::Result<Option<T>> {
    loop {
        let read = reader.take(MAX_LINE.saturating_sub(line.len() as u64)).read_line(line)?;
        if read == 0 {
            if line.len() as u64 >= MAX_LINE {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "a message was too long"));
            }
            return Ok(None);
        }
        if !line.ends_with('\n') {
            continue;
        }
        let parsed = serde_json::from_str::<T>(line.trim());
        line.clear();
        if let Ok(message) = parsed {
            return Ok(Some(message));
        }
    }
}

/// Reads the next message; `None` once the other end has closed.
pub fn read_message<T: DeserializeOwned>(reader: &mut impl BufRead) -> io::Result<Option<T>> {
    read_into(reader, &mut String::new())
}

impl Peer {
    /// Who connected, named by the system rather than by anything they sent. `None` when it can't tell.
    pub fn caller(&self) -> Option<Caller> {
        self.peer_pid().and_then(caller::identify)
    }
}

impl Client {
    /// Says who the command works for and waits briefly for the app's welcome. Returns the app's version, or what
    /// went wrong in a sentence: an app from another version, or one that doesn't answer at all (a copy from before
    /// agents' work moved into the app ignores the hello).
    pub fn hello(&mut self, client: &str, session: &str) -> Result<String, String> {
        self.hello_within(client, session, Duration::from_secs(HELLO_SECONDS))
    }

    fn hello_within(&mut self, client: &str, session: &str, wait: Duration) -> Result<String, String> {
        let hello = ToApp::Hello { protocol: PROTOCOL, client: client.into(), session: session.into() };
        self.send(&hello).map_err(|e| format!("couldn't reach the Shotmaxxing app: {e}"))?;
        let never = AtomicBool::new(false);
        match self.receive_until(Instant::now() + wait, &never) {
            Ok(Some(FromApp::Welcome { protocol: PROTOCOL, version })) => Ok(version),
            Ok(Some(FromApp::Welcome { version, .. })) => Err(format!(
                "the Shotmaxxing app that's open (version {version}) doesn't match this shotmaxxing command; update \
                 it, or quit it and open it again"
            )),
            Ok(Some(_)) => Err("the Shotmaxxing app answered out of turn".into()),
            Ok(None) => Err("the Shotmaxxing app closed the connection".into()),
            Err(_) => Err(
                "the Shotmaxxing app that's open didn't answer; it may be an older version, so quit it and \
                 open it again"
                    .into(),
            ),
        }
    }
}

#[cfg(unix)]
pub use unix::*;

#[cfg(windows)]
pub use pipe::*;

#[cfg(not(any(unix, windows)))]
pub use elsewhere::*;

/// Elsewhere the app can't be reached, and the command carries on by itself.
#[cfg(not(any(unix, windows)))]
mod elsewhere {
    use std::path::Path;

    use super::*;

    fn unsupported() -> io::Error {
        io::Error::new(io::ErrorKind::Unsupported, "the control API isn't available on this system yet")
    }

    pub struct Connection<In, Out>(PhantomData<(In, Out)>);
    pub type Client = Connection<FromApp, ToApp>;
    pub type Peer = Connection<ToApp, FromApp>;

    pub struct Sender<Out>(PhantomData<Out>);

    impl<Out: Serialize> Sender<Out> {
        pub fn send(&mut self, _message: &Out) -> io::Result<()> {
            Err(unsupported())
        }

        pub fn try_clone(&self) -> io::Result<Self> {
            Err(unsupported())
        }

        pub fn close(&self) {}
    }

    impl<In: DeserializeOwned, Out: Serialize> Connection<In, Out> {
        pub fn send(&mut self, _message: &Out) -> io::Result<()> {
            Err(unsupported())
        }

        pub fn receive(&mut self) -> io::Result<Option<In>> {
            Ok(None)
        }

        pub fn receive_until(&mut self, _deadline: Instant, _cancel: &AtomicBool) -> io::Result<Option<In>> {
            Ok(None)
        }

        pub fn sender(&self) -> io::Result<Sender<Out>> {
            Err(unsupported())
        }

        pub fn peer_pid(&self) -> Option<u32> {
            None
        }
    }

    impl Client {
        pub fn connect() -> Option<Self> {
            None
        }
    }

    pub struct Server;

    impl Server {
        pub fn bind(_path: &Path) -> io::Result<Self> {
            Err(unsupported())
        }

        pub fn run(self, _handle: impl Fn(Peer) + Send + Sync + 'static) {}
    }
}

#[cfg(unix)]
mod unix {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::Path;

    use super::*;

    /// One end of a connection: it receives `In` and sends `Out`.
    pub struct Connection<In, Out> {
        reader: BufReader<UnixStream>,
        writer: UnixStream,
        pending: String,
        kinds: PhantomData<(In, Out)>,
    }

    /// The command's end.
    pub type Client = Connection<FromApp, ToApp>;
    /// The app's end.
    pub type Peer = Connection<ToApp, FromApp>;

    /// A second handle that only sends, for another thread.
    pub struct Sender<Out> {
        writer: UnixStream,
        kind: PhantomData<Out>,
    }

    impl<Out: Serialize> Sender<Out> {
        pub fn send(&mut self, message: &Out) -> io::Result<()> {
            write_message(&mut self.writer, message)
        }

        /// Another handle on the same connection.
        pub fn try_clone(&self) -> io::Result<Self> {
            Ok(Self { writer: self.writer.try_clone()?, kind: PhantomData })
        }

        /// Closes the connection both ways, which also ends a receive waiting on another thread.
        pub fn close(&self) {
            let _ = self.writer.shutdown(std::net::Shutdown::Both);
        }
    }

    impl<In: DeserializeOwned, Out: Serialize> Connection<In, Out> {
        fn new(stream: UnixStream) -> io::Result<Self> {
            let writer = stream.try_clone()?;
            Ok(Self { reader: BufReader::new(stream), writer, pending: String::new(), kinds: PhantomData })
        }

        pub fn send(&mut self, message: &Out) -> io::Result<()> {
            write_message(&mut self.writer, message)
        }

        /// Waits for the next message; `None` once the other end has closed.
        pub fn receive(&mut self) -> io::Result<Option<In>> {
            self.wait_at_most(None)?;
            read_into(&mut self.reader, &mut self.pending)
        }

        /// Waits for the next message until `deadline`, or until `cancel` is set. Both count as a timeout.
        pub fn receive_until(&mut self, deadline: Instant, cancel: &AtomicBool) -> io::Result<Option<In>> {
            self.wait_at_most(Some(Duration::from_millis(200)))?;
            loop {
                if cancel.load(Ordering::SeqCst) || Instant::now() >= deadline {
                    return Err(io::ErrorKind::TimedOut.into());
                }
                match read_into(&mut self.reader, &mut self.pending) {
                    Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {}
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    other => return other,
                }
            }
        }

        /// A handle that sends from another thread while this one receives.
        pub fn sender(&self) -> io::Result<Sender<Out>> {
            Ok(Sender { writer: self.writer.try_clone()?, kind: PhantomData })
        }

        /// The process on the other end, as the system reports it.
        pub fn peer_pid(&self) -> Option<u32> {
            caller::peer_pid(&self.writer)
        }

        /// How long each read may wait.
        fn wait_at_most(&self, timeout: Option<Duration>) -> io::Result<()> {
            match self.reader.get_ref().set_read_timeout(timeout) {
                // macOS refuses options on a socket whose other end has gone; the read then reports the end.
                Err(e) if e.kind() == io::ErrorKind::InvalidInput => Ok(()),
                other => other,
            }
        }
    }

    impl Client {
        /// Connects to the running app. `None` when it isn't open.
        pub fn connect() -> Option<Self> {
            Self::connect_to(&socket_path()?).ok()
        }

        pub fn connect_to(path: &Path) -> io::Result<Self> {
            Self::new(UnixStream::connect(path)?)
        }
    }

    /// The app's listening socket.
    pub struct Server {
        listener: UnixListener,
    }

    impl Server {
        /// Starts listening at `path`, which only this user can open. Fails when another copy of the app is
        /// already listening there; a socket left behind by a copy that quit is replaced.
        pub fn bind(path: &Path) -> io::Result<Self> {
            if let Some(folder) = path.parent() {
                fs::create_dir_all(folder)?;
            }
            // A socket someone answers on is a running app. Asked twice more, a moment apart, because a process
            // being started anywhere in this one briefly holds a copy of every open socket, a closed listener's too,
            // until it runs its program, and a connection made in that window means nothing.
            let answers = || UnixStream::connect(path).is_ok();
            let still = |ms| {
                std::thread::sleep(std::time::Duration::from_millis(ms));
                answers()
            };
            if answers() && still(60) && still(240) {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    "another Shotmaxxing is already listening",
                ));
            }
            match fs::remove_file(path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
            let listener = UnixListener::bind(path)?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
            Ok(Self { listener })
        }

        /// Accepts connections until the socket fails, handing each to `handle` on its own thread.
        pub fn run(self, handle: impl Fn(Peer) + Send + Sync + 'static) {
            let handle = std::sync::Arc::new(handle);
            for stream in self.listener.incoming() {
                let Ok(stream) = stream else { continue };
                let Ok(peer) = Peer::new(stream) else { continue };
                let handle = handle.clone();
                std::thread::Builder::new().name("control peer".into()).spawn(move || handle(peer)).ok();
            }
        }
    }
}

/// Windows: a named pipe on this computer only, that only this user can open. The first instance claims the name, so
/// no other process can listen in the app's place, and the command talks only to a pipe served by this user.
///
/// The pipe is synchronous, and Windows lets only one call at a time into a synchronous handle: a read waiting inside
/// the pipe would hold up a send from another thread. So reads look first and only read what has arrived, napping
/// between looks (at most 50 ms) while nothing comes. That only happens while a take is connected.
#[cfg(windows)]
#[allow(unsafe_code)]
mod pipe {
    use std::fs::File;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
    use std::path::Path;
    use std::ptr::null_mut;
    use std::sync::Arc;

    use windows_sys::Win32::Foundation::{
        ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED,
        ERROR_PIPE_NOT_CONNECTED, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        EqualSid, GetTokenInformation, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, TOKEN_QUERY,
        TOKEN_USER, TokenUser,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX, SECURITY_IDENTIFICATION,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId,
        GetNamedPipeServerProcessId, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
        PIPE_UNLIMITED_INSTANCES, PIPE_WAIT, PeekNamedPipe, WaitNamedPipeW,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    use super::*;

    /// The longest nap between looks at a pipe that has nothing to read.
    const NAP: Duration = Duration::from_millis(50);
    /// Each direction's buffer.
    const BUFFER: u32 = 64 * 1024;

    /// The app's pipe for this user.
    pub(super) fn name() -> Option<PathBuf> {
        let sid = user_sid().ok()?;
        Some(PathBuf::from(format!(r"\\.\pipe\{APP_ID}.control.{sid}")))
    }

    /// One connected pipe, shared by a connection's reading and sending sides.
    struct Pipe {
        file: File,
        /// Set by [`Sender::close`]: reads end as if the other side had closed.
        closed: AtomicBool,
        /// Whether this is the app's end, which can disconnect the other side outright.
        server: bool,
    }

    impl Pipe {
        fn new(file: File, server: bool) -> Arc<Self> {
            Arc::new(Self { file, closed: AtomicBool::new(false), server })
        }

        fn handle(&self) -> HANDLE {
            self.file.as_raw_handle() as HANDLE
        }

        /// How many bytes are waiting; `None` once the other side has gone.
        fn waiting(&self) -> io::Result<Option<usize>> {
            let mut waiting = 0u32;
            // SAFETY: the handle is an open pipe; no buffer is passed, so nothing is copied out.
            let ok =
                unsafe { PeekNamedPipe(self.handle(), null_mut(), 0, null_mut(), &mut waiting, null_mut()) };
            if ok != 0 {
                return Ok(Some(waiting as usize));
            }
            let error = io::Error::last_os_error();
            if gone(&error) { Ok(None) } else { Err(error) }
        }
    }

    /// The other side closed or disconnected.
    fn gone(error: &io::Error) -> bool {
        matches!(
            error.raw_os_error().map(|code| code as u32),
            Some(ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED | ERROR_NO_DATA)
        )
    }

    /// Reads only what has arrived, waiting by napping; see the module notes.
    struct PipeReader {
        pipe: Arc<Pipe>,
        /// How long one read may wait for something to arrive, like a socket's read timeout.
        timeout: Option<Duration>,
    }

    impl Read for PipeReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let started = Instant::now();
            let mut nap = Duration::from_millis(1);
            loop {
                if self.pipe.closed.load(Ordering::SeqCst) {
                    return Ok(0);
                }
                match self.pipe.waiting()? {
                    None => return Ok(0),
                    Some(0) => {}
                    Some(waiting) => {
                        let len = waiting.min(buf.len());
                        return match (&self.pipe.file).read(&mut buf[..len]) {
                            Err(e) if gone(&e) => Ok(0),
                            other => other,
                        };
                    }
                }
                if let Some(timeout) = self.timeout {
                    let left = timeout.saturating_sub(started.elapsed());
                    if left.is_zero() {
                        return Err(io::ErrorKind::TimedOut.into());
                    }
                    nap = nap.min(left);
                }
                std::thread::sleep(nap);
                nap = (nap * 2).min(NAP);
            }
        }
    }

    struct PipeWriter(Arc<Pipe>);

    impl Write for PipeWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.0.closed.load(Ordering::SeqCst) {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            (&self.0.file).write(buf)
        }

        /// Written bytes are already in the pipe. (Flushing a pipe's file buffers would wait for the other side to
        /// read everything.)
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// One end of a connection: it receives `In` and sends `Out`.
    pub struct Connection<In, Out> {
        reader: BufReader<PipeReader>,
        writer: PipeWriter,
        pending: String,
        kinds: PhantomData<(In, Out)>,
    }

    /// The command's end.
    pub type Client = Connection<FromApp, ToApp>;
    /// The app's end.
    pub type Peer = Connection<ToApp, FromApp>;

    /// A second handle that only sends, for another thread.
    pub struct Sender<Out> {
        writer: PipeWriter,
        kind: PhantomData<Out>,
    }

    impl<Out: Serialize> Sender<Out> {
        pub fn send(&mut self, message: &Out) -> io::Result<()> {
            write_message(&mut self.writer, message)
        }

        /// Another handle on the same connection.
        pub fn try_clone(&self) -> io::Result<Self> {
            Ok(Self { writer: PipeWriter(self.writer.0.clone()), kind: PhantomData })
        }

        /// Closes the connection both ways, which also ends a receive waiting on another thread. The other side sees
        /// the end once this side's last handle goes.
        pub fn close(&self) {
            let pipe = &self.writer.0;
            pipe.closed.store(true, Ordering::SeqCst);
            if pipe.server {
                // SAFETY: disconnects a pipe instance this process created and still holds open.
                unsafe { DisconnectNamedPipe(pipe.handle()) };
            }
        }
    }

    impl<In: DeserializeOwned, Out: Serialize> Connection<In, Out> {
        fn new(pipe: Arc<Pipe>) -> Self {
            Self {
                reader: BufReader::new(PipeReader { pipe: pipe.clone(), timeout: None }),
                writer: PipeWriter(pipe),
                pending: String::new(),
                kinds: PhantomData,
            }
        }

        pub fn send(&mut self, message: &Out) -> io::Result<()> {
            write_message(&mut self.writer, message)
        }

        /// Waits for the next message; `None` once the other end has closed.
        pub fn receive(&mut self) -> io::Result<Option<In>> {
            self.reader.get_mut().timeout = None;
            read_into(&mut self.reader, &mut self.pending)
        }

        /// Waits for the next message until `deadline`, or until `cancel` is set. Both count as a timeout.
        pub fn receive_until(&mut self, deadline: Instant, cancel: &AtomicBool) -> io::Result<Option<In>> {
            self.reader.get_mut().timeout = Some(Duration::from_millis(200));
            loop {
                if cancel.load(Ordering::SeqCst) || Instant::now() >= deadline {
                    return Err(io::ErrorKind::TimedOut.into());
                }
                match read_into(&mut self.reader, &mut self.pending) {
                    Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {}
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                    other => return other,
                }
            }
        }

        /// A handle that sends from another thread while this one receives.
        pub fn sender(&self) -> io::Result<Sender<Out>> {
            Ok(Sender { writer: PipeWriter(self.writer.0.clone()), kind: PhantomData })
        }

        /// The process on the other end, as the system reports it.
        pub fn peer_pid(&self) -> Option<u32> {
            let pipe = &self.writer.0;
            let mut pid = 0u32;
            // SAFETY: asks about a pipe this process holds open.
            let ok = unsafe {
                if pipe.server {
                    GetNamedPipeClientProcessId(pipe.handle(), &mut pid)
                } else {
                    GetNamedPipeServerProcessId(pipe.handle(), &mut pid)
                }
            };
            (ok != 0 && pid != 0).then_some(pid)
        }
    }

    impl Client {
        /// Connects to the running app. `None` when it isn't open.
        pub fn connect() -> Option<Self> {
            Self::connect_to(&socket_path()?).ok()
        }

        /// Connects to the pipe at `path`, once it's clear this user's process is serving it.
        pub fn connect_to(path: &Path) -> io::Result<Self> {
            let open = || {
                std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    // The server may learn who connected, but never act as this user.
                    .security_qos_flags(SECURITY_IDENTIFICATION)
                    .open(path)
            };
            let file = match open() {
                // Every instance is taken for a moment while the app makes the next one.
                Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                    let name = wide(path);
                    // SAFETY: a NUL-terminated pipe name; waits at most a second for a free instance.
                    unsafe { WaitNamedPipeW(name.as_ptr(), 1000) };
                    open()?
                }
                other => other?,
            };
            if !served_by_this_user(&file)? {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "something other than this user's Shotmaxxing is listening on its pipe",
                ));
            }
            Ok(Self::new(Pipe::new(file, false)))
        }
    }

    /// The app's pipe, with the next instance waiting for a connection.
    pub struct Server {
        name: Vec<u16>,
        security: OnlyThisUser,
        next: OwnedHandle,
    }

    impl Server {
        /// Starts listening at `path`, which only this user can open. Fails when another copy of the app (or
        /// anything else) already holds the name.
        pub fn bind(path: &Path) -> io::Result<Self> {
            let name = wide(path);
            let security = OnlyThisUser::new()?;
            let next = instance(&name, &security, true).map_err(|e| {
                if e.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) {
                    io::Error::new(io::ErrorKind::AddrInUse, "another Shotmaxxing is already listening")
                } else {
                    e
                }
            })?;
            Ok(Self { name, security, next })
        }

        /// Accepts connections until the pipe fails, handing each to `handle` on its own thread.
        pub fn run(mut self, handle: impl Fn(Peer) + Send + Sync + 'static) {
            let handle = Arc::new(handle);
            loop {
                // SAFETY: waits for a client on an instance this server created and holds.
                let connected = unsafe { ConnectNamedPipe(self.next.as_raw_handle() as HANDLE, null_mut()) }
                    != 0
                    || io::Error::last_os_error().raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32);
                // The next instance exists before this one is handed off, so the name is never free to take.
                let Ok(fresh) = instance(&self.name, &self.security, false) else { return };
                let taken = std::mem::replace(&mut self.next, fresh);
                if !connected {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
                let peer = Peer::new(Pipe::new(File::from(taken), true));
                let handle = handle.clone();
                std::thread::Builder::new().name("control peer".into()).spawn(move || handle(peer)).ok();
            }
        }
    }

    /// Makes one instance of the pipe; `first` claims the name and fails if anything already holds it.
    fn instance(name: &[u16], security: &OnlyThisUser, first: bool) -> io::Result<OwnedHandle> {
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: security.0,
            bInheritHandle: 0,
        };
        let open = PIPE_ACCESS_DUPLEX | if first { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
        let mode = PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;
        // SAFETY: a NUL-terminated name, and attributes whose descriptor outlives the call.
        let pipe = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                open,
                mode,
                PIPE_UNLIMITED_INSTANCES,
                BUFFER,
                BUFFER,
                0,
                &attributes,
            )
        };
        if pipe == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a new handle that nothing else owns.
        Ok(unsafe { OwnedHandle::from_raw_handle(pipe as RawHandle) })
    }

    /// A security descriptor that lets this user, and no one else, open the pipe.
    struct OnlyThisUser(PSECURITY_DESCRIPTOR);

    // SAFETY: the descriptor is plain memory the system allocated for this value; nothing else refers to it.
    unsafe impl Send for OnlyThisUser {}

    impl OnlyThisUser {
        fn new() -> io::Result<Self> {
            // Protected, so nothing is inherited: full access for this user only.
            let sddl = wide(Path::new(&format!("D:P(A;;GA;;;{})", user_sid()?)));
            let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
            // SAFETY: parses a NUL-terminated string into a descriptor the system allocates; freed on drop.
            let ok = unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    null_mut(),
                )
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(descriptor))
        }
    }

    impl Drop for OnlyThisUser {
        fn drop(&mut self) {
            // SAFETY: the descriptor came from ConvertStringSecurityDescriptorToSecurityDescriptorW.
            unsafe { LocalFree(self.0) };
        }
    }

    /// Whether the process serving the pipe runs as this user.
    fn served_by_this_user(file: &File) -> io::Result<bool> {
        let mut pid = 0u32;
        // SAFETY: asks about a pipe this process holds open.
        if unsafe { GetNamedPipeServerProcessId(file.as_raw_handle() as HANDLE, &mut pid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: opens the server's process with the least access that can read its user.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a handle just opened, closed when this drops.
        let process = unsafe { OwnedHandle::from_raw_handle(process as RawHandle) };
        let theirs = User::of(process.as_raw_handle() as HANDLE)?;
        // SAFETY: the pseudo handle for this process, which needs no closing.
        let ours = User::of(unsafe { GetCurrentProcess() })?;
        // SAFETY: both SIDs point into buffers that live until the end of this function.
        Ok(unsafe { EqualSid(theirs.sid(), ours.sid()) } != 0)
    }

    /// This user's SID as text, like `S-1-5-21-…`.
    fn user_sid() -> io::Result<String> {
        // SAFETY: the pseudo handle for this process, which needs no closing.
        let user = User::of(unsafe { GetCurrentProcess() })?;
        let mut text = null_mut();
        // SAFETY: converts a valid SID into a string the system allocates, freed below.
        if unsafe { ConvertSidToStringSidW(user.sid(), &mut text) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut len = 0;
        // SAFETY: the system's string ends with a zero.
        while unsafe { *text.add(len) } != 0 {
            len += 1;
        }
        // SAFETY: `len` characters were just counted in it.
        let sid = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, len) });
        // SAFETY: the string came from ConvertSidToStringSidW.
        unsafe { LocalFree(text.cast()) };
        Ok(sid)
    }

    /// A process's user: the TOKEN_USER the system wrote, with the SID it points into.
    struct User(Vec<u64>);

    impl User {
        fn of(process: HANDLE) -> io::Result<Self> {
            let mut token: HANDLE = null_mut();
            // SAFETY: reads the token of a process handle the caller holds.
            if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: a handle just opened, closed when this drops.
            let token = unsafe { OwnedHandle::from_raw_handle(token as RawHandle) };
            let token = token.as_raw_handle() as HANDLE;
            let mut needed = 0u32;
            // SAFETY: asks for the size only.
            unsafe { GetTokenInformation(token, TokenUser, null_mut(), 0, &mut needed) };
            if needed == 0 {
                return Err(io::Error::last_os_error());
            }
            // Eight-byte cells keep the TOKEN_USER at its alignment.
            let mut cells = vec![0u64; (needed as usize).div_ceil(8)];
            // SAFETY: the buffer holds `needed` bytes.
            let ok = unsafe {
                GetTokenInformation(token, TokenUser, cells.as_mut_ptr().cast(), needed, &mut needed)
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(cells))
        }

        fn sid(&self) -> PSID {
            // SAFETY: the buffer starts with the TOKEN_USER that GetTokenInformation wrote.
            unsafe { (*self.0.as_ptr().cast::<TOKEN_USER>()).User.Sid }
        }
    }

    /// A path as the NUL-terminated wide string Windows takes.
    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
    }
}

#[cfg(all(test, any(unix, windows)))]
mod tests {
    use super::*;

    fn script() -> TakeScript {
        TakeScript::parse(r#"{"app":"TextEdit","beats":[{"type":{"text":"hi"}}]}"#).unwrap()
    }

    #[test]
    fn opening_a_project_is_one_request_and_one_answer() {
        let mut out = Vec::new();
        write_message(&mut out, &ToApp::Open { project: PathBuf::from("/Movies/Demo.smx") }).unwrap();
        write_message(&mut out, &FromApp::Opened).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "{\"type\":\"open\",\"project\":\"/Movies/Demo.smx\"}\n{\"type\":\"opened\"}\n"
        );
    }

    #[test]
    fn messages_are_tagged_json_lines() {
        let mut out = Vec::new();
        write_message(&mut out, &FromApp::Answer { allowed: false, reason: Some("Not now".into()) }).unwrap();
        write_message(&mut out, &FromApp::Progress { done: 0.5, message: "Typing".into() }).unwrap();
        let text = String::from_utf8(out.clone()).unwrap();
        assert_eq!(
            text,
            "{\"type\":\"answer\",\"allowed\":false,\"reason\":\"Not now\"}\n\
             {\"type\":\"progress\",\"done\":0.5,\"message\":\"Typing\"}\n"
        );
        // Kinds from a newer version are skipped, and blank lines too.
        let input = format!("{{\"type\":\"wave\"}}\n\n{text}");
        let mut reader = io::Cursor::new(input.into_bytes());
        assert_eq!(
            read_message::<FromApp>(&mut reader).unwrap(),
            Some(FromApp::Answer { allowed: false, reason: Some("Not now".into()) })
        );
        assert_eq!(
            read_message::<FromApp>(&mut reader).unwrap(),
            Some(FromApp::Progress { done: 0.5, message: "Typing".into() })
        );
        assert_eq!(read_message::<FromApp>(&mut reader).unwrap(), None);
        // A take carries its script whole.
        let take = ToApp::Take {
            take: Box::new(TakeRequest {
                script: script(),
                planned: Vec::new(),
                beats: vec!["Type “hi”".into()],
                seconds: 2.0,
                folder: PathBuf::from("/tmp/takes"),
                microphone: None,
                system_audio: false,
                plan: Some(TakePlan { title: "Say hi".into(), steps: vec!["Type a greeting".into()] }),
            }),
        };
        let mut line = Vec::new();
        write_message(&mut line, &take).unwrap();
        assert_eq!(read_message::<ToApp>(&mut io::Cursor::new(line)).unwrap(), Some(take));
    }

    #[test]
    fn pictures_are_base64_jpegs_with_their_size() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        // SOI, an APP0 segment, then a baseline frame header for 640×360.
        let jpeg = [
            0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x01, 0x68, 0x02,
            0x80, 0x03,
        ];
        let picture = Picture::from_jpeg(&jpeg);
        assert_eq!((picture.width, picture.height), (640, 360));
        assert_eq!(picture.data, base64(&jpeg));
        assert_eq!(jpeg_size(b"not a jpeg at all"), None);
    }

    #[test]
    fn a_take_goes_back_and_forth_over_the_socket() {
        let dir = tempfile::tempdir().unwrap();
        let path = scratch_address(dir.path());
        let server = Server::bind(&path).unwrap();
        // Only this user may connect.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        // A second copy can't take over a live socket.
        assert_eq!(Server::bind(&path).err().map(|e| e.kind()), Some(io::ErrorKind::AddrInUse));

        std::thread::spawn(move || {
            server.run(|mut peer| {
                // The system says who connected: here, this very process.
                let who = peer.caller().map_or_else(|| "unknown".to_string(), |c| c.pid.to_string());
                while let Ok(Some(message)) = peer.receive() {
                    match message {
                        ToApp::Hello { protocol, .. } => {
                            peer.send(&FromApp::Welcome { protocol, version: who.clone() }).unwrap()
                        }
                        ToApp::Take { take } => {
                            let allowed = take.script.app.as_deref() == Some("TextEdit");
                            peer.send(&FromApp::Answer { allowed, reason: None }).unwrap();
                            peer.send(&FromApp::Progress { done: 0.5, message: "Typing".into() }).unwrap();
                        }
                        ToApp::Cancel => {
                            let performed = Performed {
                                project: PathBuf::from("/tmp/takes/Take.smx"),
                                beats: 1,
                                stopped: Some("Called off".into()),
                                ..Performed::default()
                            };
                            peer.send(&FromApp::Performed { performed }).unwrap();
                        }
                        _ => {}
                    }
                }
            })
        });

        let mut client = Client::connect_to(&path).unwrap();
        let version = client.hello("Claude Code", "session-1").unwrap();
        if cfg!(any(target_os = "macos", windows)) {
            assert_eq!(version, std::process::id().to_string(), "the app sees who connected");
        }
        let take = TakeRequest {
            script: script(),
            planned: Vec::new(),
            beats: vec!["Type “hi”".into()],
            seconds: 2.0,
            folder: PathBuf::from("/tmp/takes"),
            microphone: None,
            system_audio: false,
            plan: None,
        };
        client.send(&ToApp::Take { take: Box::new(take) }).unwrap();
        let never = AtomicBool::new(false);
        let deadline = Instant::now() + Duration::from_secs(5);
        assert_eq!(
            client.receive_until(deadline, &never).unwrap(),
            Some(FromApp::Answer { allowed: true, reason: None })
        );
        assert_eq!(
            client.receive_until(deadline, &never).unwrap(),
            Some(FromApp::Progress { done: 0.5, message: "Typing".into() })
        );
        // Nothing more comes until the take moves on: waiting ends at the deadline.
        let soon = Instant::now() + Duration::from_millis(300);
        assert_eq!(client.receive_until(soon, &never).err().map(|e| e.kind()), Some(io::ErrorKind::TimedOut));
        let mut sender = client.sender().unwrap();
        sender.send(&ToApp::Cancel).unwrap();
        match client.receive().unwrap() {
            Some(FromApp::Performed { performed }) => {
                assert_eq!(performed.stopped.as_deref(), Some("Called off"))
            }
            other => panic!("expected how the take went, got {other:?}"),
        }
    }

    #[test]
    fn an_app_that_ignores_the_hello_is_named_as_out_of_date() {
        let dir = tempfile::tempdir().unwrap();
        let path = scratch_address(dir.path());
        let server = Server::bind(&path).unwrap();
        // A copy from before protocol 2 skips kinds it doesn't know, so it never answers.
        std::thread::spawn(move || server.run(|mut peer| while let Ok(Some(_)) = peer.receive() {}));
        let mut client = Client::connect_to(&path).unwrap();
        let error = client.hello_within("Codex", "s", Duration::from_millis(300)).unwrap_err();
        assert!(error.contains("older version"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn a_socket_left_behind_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control.sock");
        drop(Server::bind(&path).unwrap());
        assert!(path.exists(), "a quit app leaves its socket");
        let server = Server::bind(&path).expect("nobody listens there any more");
        drop(server);
    }

    #[cfg(unix)]
    #[test]
    fn the_socket_lives_in_the_apps_folder() {
        if let Some(path) = socket_path() {
            assert!(path.ends_with(format!("{APP_ID}/control.sock")), "{}", path.display());
        }
    }

    #[cfg(windows)]
    #[test]
    fn the_pipe_is_named_for_the_app_and_this_user() {
        let path = default_path().expect("this user has a SID");
        let name = path.to_string_lossy();
        assert!(name.starts_with(&format!(r"\\.\pipe\{APP_ID}.control.S-1-")), "{name}");
    }

    #[test]
    fn a_closed_app_ends_the_request_and_a_quit_app_frees_its_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = scratch_address(dir.path());
        let server = Server::bind(&path).unwrap();
        std::thread::spawn(move || {
            // The app answers the hello, then goes away.
            server.run(|mut peer| {
                if let Ok(Some(ToApp::Hello { protocol, .. })) = peer.receive() {
                    peer.send(&FromApp::Welcome { protocol, version: "1.0".into() }).unwrap();
                }
            })
        });
        let mut client = Client::connect_to(&path).unwrap();
        assert_eq!(client.hello("Cursor", "s").as_deref(), Ok("1.0"));
        assert_eq!(client.receive().unwrap(), None, "the app's end closed");

        // Closing our own end ends a receive waiting on another thread.
        let mut client = Client::connect_to(&path).unwrap();
        let sender = client.sender().unwrap();
        let waiting = std::thread::spawn(move || client.receive().map(|m| m.is_none()));
        std::thread::sleep(Duration::from_millis(100));
        sender.close();
        assert!(waiting.join().unwrap().unwrap_or(true), "the waiting receive ended");

        // Nobody listens at a fresh address.
        let elsewhere = scratch_address(tempfile::tempdir().unwrap().path());
        assert!(Client::connect_to(&elsewhere).is_err());
    }
}
