//! The control API: how the `shotmaxxing` command, and through it every connected agent, reaches the running app.
//!
//! The app listens on a Unix socket in its own support folder, which only this user can open. The command connects
//! when it has something to show the person, sends one JSON message per line, and carries on by itself when the app
//! isn't open. Today it carries agent takes: the app asks the person before a take starts, then shows it live on the
//! take card, where Stop and Hand back reach the take straight away.

#![forbid(unsafe_code)]

use std::io::{self, BufRead, BufReader, Read, Write};
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub use shotmaxxing_core::APP_ID;

/// The longest message accepted, so a confused peer can't fill memory.
const MAX_LINE: u64 = 1 << 20;

/// A take an agent wants to film, as the person sees it before saying yes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TakeRequest {
    /// The agent, e.g. "Claude Code".
    pub client: String,
    /// The agent's session (one `shotmaxxing mcp` process), so "allow for this session" can be remembered.
    pub session: String,
    /// The app being filmed.
    pub app: String,
    /// Each beat in words.
    pub beats: Vec<String>,
    /// About how long the take runs, in seconds.
    pub seconds: f64,
}

/// How a take ended.
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

/// From the command to the app.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToApp {
    /// Asks the person before a take. The app answers with [`FromApp::Answer`].
    Ask { take: TakeRequest },
    /// The take is on beat `beat` (counting from 0), `done` of the way through (0 to 1).
    Progress { beat: usize, done: f64 },
    /// The take is waiting for the person to do something, then hand back.
    Waiting { message: String },
    /// The take finished, or stopped early.
    Finished { result: TakeResult },
}

/// From the app to the command.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FromApp {
    /// The person's answer to [`ToApp::Ask`], with the reason when it's no.
    Answer {
        allowed: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// The person pressed Stop on the take card.
    Stop,
    /// The person pressed Hand back on the take card.
    HandBack,
}

/// Where the app listens: `~/Library/Application Support/<APP_ID>/control.sock` on macOS, and the runtime folder (or
/// the data folder) on Linux. `None` on Windows for now, without a home folder, or when the path is too long for a
/// socket. `SHOTMAXXING_CONTROL_SOCKET` points both ends somewhere else, for tests and development.
pub fn socket_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SHOTMAXXING_CONTROL_SOCKET").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(path));
    }
    let path = if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support").join(APP_ID)
    } else if cfg!(unix) {
        match std::env::var_os("XDG_RUNTIME_DIR") {
            Some(runtime) => PathBuf::from(runtime).join(APP_ID),
            None => PathBuf::from(std::env::var_os("HOME")?).join(".local/share").join(APP_ID),
        }
    } else {
        return None;
    }
    .join("control.sock");
    // Socket paths are limited to 104 bytes on macOS, including the final zero.
    (path.as_os_str().len() < 104).then_some(path)
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

#[cfg(unix)]
pub use unix::*;

#[cfg(not(unix))]
pub use elsewhere::*;

/// Without Unix sockets (Windows, until named pipes are wired up) the app can't be reached, and the command carries
/// on by itself.
#[cfg(not(unix))]
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
            self.reader.get_ref().set_read_timeout(None)?;
            read_into(&mut self.reader, &mut self.pending)
        }

        /// Waits for the next message until `deadline`, or until `cancel` is set. Both count as a timeout.
        pub fn receive_until(&mut self, deadline: Instant, cancel: &AtomicBool) -> io::Result<Option<In>> {
            self.reader.get_ref().set_read_timeout(Some(Duration::from_millis(200)))?;
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
            if UnixStream::connect(path).is_ok() {
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn messages_are_tagged_json_lines() {
        let mut out = Vec::new();
        write_message(&mut out, &FromApp::Answer { allowed: false, reason: Some("Not now".into()) }).unwrap();
        write_message(&mut out, &FromApp::Stop).unwrap();
        let text = String::from_utf8(out.clone()).unwrap();
        assert_eq!(
            text,
            "{\"type\":\"answer\",\"allowed\":false,\"reason\":\"Not now\"}\n{\"type\":\"stop\"}\n"
        );
        // Kinds from a newer version are skipped, and blank lines too.
        let input = format!("{{\"type\":\"wave\"}}\n\n{text}");
        let mut reader = io::Cursor::new(input.into_bytes());
        assert_eq!(
            read_message::<FromApp>(&mut reader).unwrap(),
            Some(FromApp::Answer { allowed: false, reason: Some("Not now".into()) })
        );
        assert_eq!(read_message::<FromApp>(&mut reader).unwrap(), Some(FromApp::Stop));
        assert_eq!(read_message::<FromApp>(&mut reader).unwrap(), None);
    }

    #[test]
    fn a_take_goes_back_and_forth_over_the_socket() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control.sock");
        let server = Server::bind(&path).unwrap();
        // Only this user may connect.
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        // A second copy can't take over a live socket.
        assert_eq!(Server::bind(&path).err().map(|e| e.kind()), Some(io::ErrorKind::AddrInUse));

        std::thread::spawn(move || {
            server.run(|mut peer| {
                while let Ok(Some(message)) = peer.receive() {
                    match message {
                        ToApp::Ask { take } => {
                            let allowed = take.client == "Claude Code";
                            peer.send(&FromApp::Answer { allowed, reason: None }).unwrap();
                        }
                        ToApp::Progress { beat: 1, .. } => peer.send(&FromApp::Stop).unwrap(),
                        _ => {}
                    }
                }
            })
        });

        let mut client = Client::connect_to(&path).unwrap();
        let take = TakeRequest {
            client: "Claude Code".into(),
            beats: vec!["Click Export".into()],
            ..Default::default()
        };
        client.send(&ToApp::Ask { take }).unwrap();
        let never = AtomicBool::new(false);
        let deadline = Instant::now() + Duration::from_secs(5);
        assert_eq!(
            client.receive_until(deadline, &never).unwrap(),
            Some(FromApp::Answer { allowed: true, reason: None })
        );
        // Nothing more comes until the take moves on: waiting ends at the deadline.
        let soon = Instant::now() + Duration::from_millis(300);
        assert_eq!(client.receive_until(soon, &never).err().map(|e| e.kind()), Some(io::ErrorKind::TimedOut));
        let mut sender = client.sender().unwrap();
        sender.send(&ToApp::Progress { beat: 1, done: 0.5 }).unwrap();
        assert_eq!(client.receive().unwrap(), Some(FromApp::Stop));
    }

    #[test]
    fn a_socket_left_behind_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("control.sock");
        drop(Server::bind(&path).unwrap());
        assert!(path.exists(), "a quit app leaves its socket");
        let server = Server::bind(&path).expect("nobody listens there any more");
        drop(server);
    }

    #[test]
    fn the_socket_lives_in_the_apps_folder() {
        if let Some(path) = socket_path() {
            assert!(path.ends_with(format!("{APP_ID}/control.sock")), "{}", path.display());
        }
    }
}
