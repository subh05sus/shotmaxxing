//! Talks to `shotmaxxing mcp` the way an agent does: JSON-RPC over its stdin and stdout. Nothing is recorded; the
//! export uses a test pattern from ffmpeg, and is skipped when ffmpeg isn't installed.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};
use shotmaxxing_core::bundle::ProjectBundle;
use shotmaxxing_core::geometry::Size;
use shotmaxxing_core::project::{MediaTrack, Project, TrackKind};

struct Server {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next: u64,
}

impl Server {
    /// Starts the server out of reach of a Shotmaxxing app running on this computer.
    fn start() -> Self {
        Self::start_with(&std::env::temp_dir().join(format!("no-shotmaxxing-{}.sock", std::process::id())))
    }

    /// Starts the server with `socket` as the app's control socket.
    fn start_with(socket: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_shotmaxxing"))
            .arg("mcp")
            .env("SHOTMAXXING_CONTROL_SOCKET", socket)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self { child, input, output, next: 1 }
    }

    fn send(&mut self, message: &Value) {
        writeln!(self.input, "{message}").unwrap();
        self.input.flush().unwrap();
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
    }

    /// Sends a request and returns its response, collecting any notifications that arrive first.
    fn request(&mut self, method: &str, params: Value) -> (Value, Vec<Value>) {
        let id = self.next;
        self.next += 1;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        let mut notes = Vec::new();
        loop {
            let message = self.read();
            if message.get("id") == Some(&json!(id)) {
                return (message, notes);
            }
            notes.push(message);
        }
    }

    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        let (reply, _) = self.request("tools/call", json!({ "name": name, "arguments": arguments }));
        reply["result"].clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn has_ffmpeg() -> bool {
    Command::new("ffmpeg").arg("-version").output().is_ok_and(|o| o.status.success())
}

/// A one-second project. With `media`, its screen recording is a real (generated) video.
fn make_project(dir: &Path, media: bool) -> PathBuf {
    let mut project = Project { name: "Agent take".into(), ..Project::default() };
    project.source.size = Size::new(800.0, 500.0);
    project.source.scale = 1.0;
    project.tracks = vec![MediaTrack {
        id: "screen".into(),
        kind: TrackKind::Screen,
        path: "media/screen.mp4".into(),
        duration_us: 1_000_000,
        offset_us: 0,
        volume: 1.0,
        muted: false,
        name: None,
    }];
    let bundle = ProjectBundle::create(dir, &mut project).unwrap();
    if media {
        let status = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=800x500:rate=30:duration=1"])
            .args(["-c:v", "hevc_videotoolbox", "-tag:v", "hvc1", "-b:v", "4M"])
            .arg(bundle.media_dir().join("screen.mp4"))
            .status()
            .unwrap();
        assert!(status.success());
    }
    bundle.root().to_path_buf()
}

/// The app asks the person before an agent's take. When they say no, nothing moves and the agent hears why.
#[test]
fn a_take_the_person_declines_in_the_app_never_starts() {
    use shotmaxxing_control::{FromApp, ToApp};
    let dir = tempfile::tempdir().unwrap();
    let socket = shotmaxxing_control::scratch_address(dir.path());
    let app = shotmaxxing_control::Server::bind(&socket).unwrap();
    let (asked, questions) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        app.run(move |mut peer| {
            if let Ok(Some(ToApp::Ask { take })) = peer.receive() {
                asked.send(take).unwrap();
                peer.send(&FromApp::Answer { allowed: false, reason: Some("Not today.".into()) }).unwrap();
            }
        })
    });

    let mut server = Server::start_with(&socket);
    let client = json!({ "name": "test-agent", "title": "Test agent", "version": "1" });
    server.request(
        "initialize",
        json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": client }),
    );
    let yaml = "app: Notes\nbeats:\n  - click: { point: { x: 400, y: 300 } }\n  - type: hello\n";
    let result = server.tool("perform_take", json!({ "script": yaml }));
    assert_eq!(result["isError"], json!(true), "{result}");
    assert!(result["content"][0]["text"].as_str().is_some_and(|t| t.contains("Not today.")), "{result}");

    let take = questions.recv_timeout(std::time::Duration::from_secs(5)).expect("the app was never asked");
    assert_eq!((take.client.as_str(), take.app.as_str()), ("Test agent", "Notes"));
    assert_eq!(take.beats.len(), 2);
    assert!(take.seconds > 0.5, "{take:?}");
    assert!(!take.session.is_empty());
}

#[test]
fn speaks_mcp() {
    let mut server = Server::start();
    let (reply, _) = server.request(
        "initialize",
        json!({ "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } }),
    );
    let result = &reply["result"];
    assert_eq!(result["protocolVersion"], "2025-03-26", "answers in the client's version");
    assert_eq!(result["serverInfo"]["name"], "shotmaxxing");
    assert!(result["capabilities"]["tools"].is_object());
    server.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));

    let (reply, _) = server.request("ping", json!({}));
    assert_eq!(reply["result"], json!({}));

    let (reply, _) = server.request("tools/list", json!({}));
    let names: Vec<&str> =
        reply["result"]["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
    for expected in [
        "start_recording",
        "stop_recording",
        "export_project",
        "cut_silences",
        "validate_take",
        "prepare_take",
        "perform_take",
    ] {
        assert!(names.contains(&expected), "{expected} is missing from {names:?}");
    }
    // perform_take carries the take script's schema, with its definitions where the refs point.
    let perform =
        reply["result"]["tools"].as_array().unwrap().iter().find(|t| t["name"] == "perform_take").unwrap();
    assert!(perform["inputSchema"]["$defs"]["TakeScript"]["properties"]["beats"].is_object(), "{perform}");
    assert!(perform["inputSchema"]["$defs"]["Beat"].is_object());

    let (reply, _) = server.request("no/such/method", json!({}));
    assert_eq!(reply["error"]["code"], -32601);

    // A take script, checked.
    let script = json!({ "app": "Notes", "beats": [{ "click": { "point": { "x": 10, "y": 20 } } }, { "key": "cmd+s" }] });
    let result = server.tool("validate_take", json!({ "script": script }));
    assert_eq!(result["structuredContent"]["valid"], true, "{result}");
    assert_eq!(result["structuredContent"]["beats"], 2);

    // A dry run works out the timeline and touches nothing.
    let yaml =
        "app: Notes\nbeats:\n  - click: { point: { x: 400, y: 300 } }\n  - type: hello\n  - key: cmd+s\n";
    let result = server.tool("perform_take", json!({ "script": yaml, "dry_run": true }));
    let plan = &result["structuredContent"];
    assert_eq!(plan["beats"].as_array().map(Vec::len), Some(3), "{result}");
    assert!(plan["seconds"].as_f64().is_some_and(|s| s > 1.0 && s < 10.0), "{result}");

    // Tool failures come back as results marked as errors, in words.
    let result = server.tool("project_info", json!({ "project": "/nowhere/at/all.smx" }));
    assert_eq!(result["isError"], true);
    assert!(result["content"][0]["text"].as_str().unwrap().contains("couldn't open"));
    let result = server.tool("stop_recording", json!({}));
    assert_eq!(result["isError"], true);
    let result = server.tool("recording_status", json!({}));
    assert_eq!(result["structuredContent"]["recording"], false);

    // A project: read, styled, and read again.
    let dir = tempfile::tempdir().unwrap();
    let root = make_project(dir.path(), has_ffmpeg());
    let result = server.tool("project_info", json!({ "project": root }));
    assert_eq!(result["structuredContent"]["name"], "Agent take", "{result}");
    assert_eq!(result["structuredContent"]["recorded_seconds"], 1.0);
    let result = server.tool("style_project", json!({ "project": root, "aspect": "1:1", "padding": 0.12 }));
    assert_eq!(result["structuredContent"]["aspect"], "1:1", "{result}");
    let result = server.tool("style_project", json!({ "project": root, "wallpaper": "no-such-wallpaper" }));
    assert_eq!(result["isError"], true);
    // No microphone, so no pauses to cut, and nothing changes.
    let result = server.tool("cut_silences", json!({ "project": root }));
    assert_eq!(result["structuredContent"]["pauses_cut"], 0, "{result}");

    // Music: added (loud, with no voice to sit under), turned down, and taken out.
    if cfg!(target_os = "macos") {
        let song = dir.path().join("Theme.wav");
        std::fs::write(&song, shotmaxxing_render::click::wav(&vec![0.1f32; 96_000], 48_000)).unwrap();
        let result = server.tool("set_music", json!({ "project": root, "file": song }));
        let music = &result["structuredContent"]["music"];
        assert_eq!(
            (music["name"].as_str(), music["volume"].as_f64()),
            (Some("Theme.wav"), Some(0.8)),
            "{result}"
        );
        assert_eq!(music["seconds"], 2.0);
        let result = server.tool("set_music", json!({ "project": root, "volume": 0.4, "start": 0.5 }));
        assert_eq!(result["structuredContent"]["music"]["volume"], 0.4, "{result}");
        let (_, saved) = ProjectBundle::open(&root).unwrap();
        let tracks: Vec<_> = saved.tracks.iter().filter(|t| t.kind == TrackKind::Music).collect();
        assert_eq!(tracks.len(), 1);
        assert_eq!((tracks[0].offset_us, tracks[0].path.as_str()), (500_000, "media/music.wav"));
        let result = server.tool("set_music", json!({ "project": root, "remove": true }));
        assert_eq!(result["structuredContent"]["music"], Value::Null, "{result}");
        let result = server.tool("set_music", json!({ "project": root, "volume": 0.5 }));
        assert_eq!(result["isError"], true, "{result}");
    }

    if has_ffmpeg() {
        // Exported square, with progress along the way.
        let output = dir.path().join("agent.mp4");
        let (reply, notes) = server.request(
            "tools/call",
            json!({
                "name": "export_project",
                "arguments": { "project": root, "size": 480, "fps": 30, "output": output },
                "_meta": { "progressToken": "export-1" },
            }),
        );
        let result = &reply["result"]["structuredContent"];
        assert_eq!((result["width"].as_u64(), result["height"].as_u64()), (Some(480), Some(480)), "{reply}");
        assert!(output.exists());
        let progress: Vec<f64> = notes
            .iter()
            .filter(|n| n["method"] == "notifications/progress" && n["params"]["progressToken"] == "export-1")
            .filter_map(|n| n["params"]["progress"].as_f64())
            .collect();
        assert!(progress.len() >= 2 && progress.windows(2).all(|w| w[0] <= w[1]), "{progress:?}");
        assert_eq!(progress.last(), Some(&100.0));
    }
}
