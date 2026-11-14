//! `shotmaxxing mcp`: Shotmaxxing as an MCP server for AI agents, over stdin and stdout.
//!
//! Agents such as Claude Code, Codex and Cursor start it and call its tools: see what can be recorded, record a
//! take, read what came out, tidy it up (cut silences, clean up the voice, plan zooms, style the canvas) and export
//! it. Messages are JSON-RPC 2.0, one per line. Every tool call runs on a thread of its own, so the server keeps
//! answering (and can be told to cancel) while a take records or an export renders. Logs go to stderr; stdout
//! carries nothing but protocol messages.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use shotmaxxing_capture::{DeviceChoice, FinishedProject, ProjectOptions, ProjectRecording, Target};
use shotmaxxing_core::bundle::{ProjectBundle, sanitize_file_name};
use shotmaxxing_core::project::ExportPreset;
use shotmaxxing_core::project::{AspectRatio, Background, Project, TimeRange, ZoomDepth};
use shotmaxxing_core::take::{OutputMode, TakeScript};
use shotmaxxing_render::{ExportOptions, Format, RenderProject, default_bitrate, output_size, unique_path};

/// Protocol versions this server speaks, newest first.
const PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Shotmaxxing records the screen into editable projects (.smx folders) and exports \
polished videos: a wallpaper behind the window, smooth automatic zooms on clicks and typing, a clean cursor, and \
the voice and camera when recorded. The best demos are planned takes: prepare_take lists an app's controls and \
shows its window, then perform_take films a script while Shotmaxxing moves the pointer and types at a human \
pace (try dry_run first). To film something else doing the work: list_sources, start_recording, the demo, \
stop_recording. Then export_project. project_info shows what a project holds; cut_silences, clean_voice, \
plan_zooms and style_project tidy it up before exporting. Recording needs the Screen Recording permission, and \
planned takes also Accessibility, for the app that started this server.";

type Reply = Result<Value, String>;

/// Writes protocol messages to stdout, one per line, from any thread.
#[derive(Clone)]
struct Outbox(Arc<Mutex<std::io::Stdout>>);

impl Outbox {
    fn send(&self, message: &Value) {
        if let Ok(mut out) = self.0.lock() {
            let _ = writeln!(out, "{message}");
            let _ = out.flush();
        }
    }

    fn result(&self, id: &Value, result: Value) {
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    }

    fn error(&self, id: &Value, code: i64, message: &str) {
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }));
    }
}

/// A take being recorded, on a thread of its own: capture objects stay on the thread that made them.
struct Session {
    root: PathBuf,
    started: Instant,
    finish: Sender<bool>,
    done: Receiver<Result<Option<FinishedProject>, String>>,
}

impl Session {
    /// Stops (keeping or discarding the take) and waits for the files to be finished.
    fn end(self, keep: bool) -> Result<Option<FinishedProject>, String> {
        let _ = self.finish.send(keep);
        self.done.recv_timeout(Duration::from_secs(90)).map_err(|_| {
            "saving the recording took too long; it may still appear in the folder".to_string()
        })?
    }
}

#[derive(Clone, Default)]
struct State {
    recording: Arc<Mutex<Option<Session>>>,
    /// Cancel flags of the tool calls in progress, by request id.
    calls: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    /// What the last `prepare_take` found, to check a take's refs against.
    prepared: Arc<Mutex<Option<shotmaxxing_agent::Prepared>>>,
    /// A take is being performed.
    taking: Arc<AtomicBool>,
    /// The agent's name, from `initialize`, recorded in the projects it films.
    client: Arc<Mutex<Option<String>>>,
}

/// Marks a take as running for as long as it lives.
struct Taking(Arc<AtomicBool>);

impl Drop for Taking {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Runs the server until the client closes stdin. A take still recording then is kept.
pub fn serve() -> anyhow::Result<()> {
    // Recordings start on other threads; the window server connection has to come from this one.
    shotmaxxing_capture::prepare();
    let out = Outbox(Arc::new(Mutex::new(std::io::stdout())));
    let state = State::default();
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => batch.into_iter().for_each(|m| handle(&out, &state, m)),
            Ok(message) => handle(&out, &state, message),
            Err(_) => out.error(&Value::Null, -32700, "that line isn't JSON"),
        }
    }
    let session = state.recording.lock().ok().and_then(|mut slot| slot.take());
    if let Some(session) = session
        && let Err(error) = session.end(true)
    {
        tracing::warn!(%error, "couldn't finish the recording as the client left");
    }
    Ok(())
}

fn handle(out: &Outbox, state: &State, message: Value) {
    let Some(method) = message.get("method").and_then(Value::as_str) else { return };
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let Some(id) = message.get("id").cloned() else {
        // A notification: only cancellation needs doing.
        if method == "notifications/cancelled"
            && let Some(request) = params.get("requestId")
            && let Ok(calls) = state.calls.lock()
            && let Some(flag) = calls.get(&request.to_string())
        {
            flag.store(true, Ordering::SeqCst);
        }
        return;
    };
    match method {
        "initialize" => {
            if let Some(name) = params.pointer("/clientInfo/title").or(params.pointer("/clientInfo/name"))
                && let Some(name) = name.as_str()
                && let Ok(mut client) = state.client.lock()
            {
                *client = Some(name.to_string());
            }
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
            let version = PROTOCOLS.iter().find(|v| **v == asked).unwrap_or(&PROTOCOLS[0]);
            out.result(
                &id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "shotmaxxing", "title": "Shotmaxxing", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }),
            );
        }
        "ping" => out.result(&id, json!({})),
        "tools/list" => out.result(&id, json!({ "tools": tools() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            let token = params.get("_meta").and_then(|m| m.get("progressToken")).cloned();
            let cancel = Arc::new(AtomicBool::new(false));
            if let Ok(mut calls) = state.calls.lock() {
                calls.insert(id.to_string(), cancel.clone());
            }
            let (out, state) = (out.clone(), state.clone());
            std::thread::spawn(move || {
                let progress = |done: f64, message: &str| {
                    if let Some(token) = &token {
                        out.send(&json!({
                            "jsonrpc": "2.0",
                            "method": "notifications/progress",
                            "params": { "progressToken": token, "progress": (done * 100.0).round(), "total": 100, "message": message },
                        }));
                    }
                };
                let reply = call(&state, &name, &args, &progress, &cancel);
                if let Ok(mut calls) = state.calls.lock() {
                    calls.remove(&id.to_string());
                }
                // A cancelled request gets no answer, as the protocol asks.
                if cancel.load(Ordering::SeqCst) {
                    return;
                }
                out.result(
                    &id,
                    match reply {
                        Ok(mut value) => {
                            // A picture travels as its own content item, not inside the JSON.
                            let image = value.as_object_mut().and_then(|o| o.remove("_image"));
                            let text = serde_json::to_string_pretty(&value).unwrap_or_default();
                            let mut content = vec![json!({ "type": "text", "text": text })];
                            content.extend(image);
                            json!({ "content": content, "structuredContent": value })
                        }
                        Err(message) => {
                            json!({ "content": [{ "type": "text", "text": message }], "isError": true })
                        }
                    },
                );
            });
        }
        _ => out.error(&id, -32601, &format!("there's no method called {method}")),
    }
}

/// Every tool, with what it does and what it takes.
fn tools() -> Value {
    let project =
        json!({ "type": "string", "description": "A project folder (.smx), as returned by stop_recording." });
    let wallpapers: Vec<String> =
        shotmaxxing_render::wallpaper::wallpapers().into_iter().map(|w| w.id).collect();
    json!([
        {
            "name": "list_sources",
            "title": "List screens and windows",
            "description": "The screens and windows that can be recorded, with their ids and sizes in points.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": { "readOnlyHint": true, "openWorldHint": false },
        },
        {
            "name": "list_devices",
            "title": "List microphones and cameras",
            "description": "The microphones and cameras that can be recorded alongside the screen.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": { "readOnlyHint": true, "openWorldHint": false },
        },
        {
            "name": "start_recording",
            "title": "Start recording",
            "description": "Starts recording a screen or a window into a new project, with clicks, typing and the cursor path for automatic zooms. The real cursor is left out of the video and drawn cleanly later. One take records at a time; it stops by itself after max_seconds.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "display": { "type": "integer", "description": "A display id from list_sources. With no display, window or app, the main display is recorded." },
                    "window": { "type": "integer", "description": "A window id from list_sources." },
                    "app": { "type": "string", "description": "Record the largest window of this app, e.g. \"Safari\"." },
                    "name": { "type": "string", "description": "The project's name.", "default": "Recording" },
                    "folder": { "type": "string", "description": "Where the project goes. Defaults to Movies/Shotmaxxing." },
                    "fps": { "type": "integer", "minimum": 15, "maximum": 120, "default": 60, "description": "The most frames per second; a still screen stores none." },
                    "system_audio": { "type": "boolean", "default": false, "description": "Record what the computer plays." },
                    "microphone": { "type": "string", "description": "A microphone id from list_devices, or \"default\". Leave out for no microphone." },
                    "camera": { "type": "string", "description": "A camera id from list_devices, or \"default\". Leave out for no camera." },
                    "hide_desktop_icons": { "type": "boolean", "default": false },
                    "max_seconds": { "type": "number", "minimum": 1, "default": 900, "description": "Stop by itself after this long." },
                },
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
        },
        {
            "name": "recording_status",
            "title": "Recording status",
            "description": "Whether a take is recording, which project it goes into and for how long it has run.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": { "readOnlyHint": true, "openWorldHint": false },
        },
        {
            "name": "stop_recording",
            "title": "Stop recording",
            "description": "Stops the take, plans automatic zooms and saves the project. Returns what it holds: length, zooms, tracks, and anything that went wrong, such as a microphone that wasn't allowed.",
            "inputSchema": {
                "type": "object",
                "properties": { "discard": { "type": "boolean", "default": false, "description": "Throw the take away instead of keeping it." } },
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
        },
        {
            "name": "project_info",
            "title": "Project info",
            "description": "What a project holds: its length, size, zooms, tracks, cuts, voice settings and canvas.",
            "inputSchema": { "type": "object", "properties": { "project": project }, "required": ["project"] },
            "annotations": { "readOnlyHint": true, "openWorldHint": false },
        },
        {
            "name": "export_project",
            "title": "Export a video",
            "description": "Renders the project with every edit applied, on the GPU: MP4 (H.264) plays everywhere, HEVC is smaller, GIF loops in chats and docs. Reports progress and can be cancelled.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "format": { "type": "string", "enum": ["mp4", "hevc", "gif"], "default": "mp4" },
                    "size": { "type": "integer", "minimum": 240, "maximum": 4320, "description": "The shorter side in pixels. Defaults to 1080, or 480 for a GIF." },
                    "fps": { "type": "integer", "minimum": 1, "maximum": 120, "description": "Defaults to 60, or 15 for a GIF." },
                    "output": { "type": "string", "description": "The file to write. Defaults to a new file next to the project." },
                },
                "required": ["project"],
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
        },
        {
            "name": "cut_silences",
            "title": "Cut silences",
            "description": "Shortens pauses in the voice longer than a second where nothing happens on screen (no clicks or keys, the pointer still, the screen not changing). Each keeps a short breath at both ends. Needs a microphone track.",
            "inputSchema": { "type": "object", "properties": { "project": project }, "required": ["project"] },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
        },
        {
            "name": "clean_voice",
            "title": "Clean up the voice",
            "description": "Turns down hum, fan noise and hiss in the microphone recording and evens out its level. The original stays; the export uses the cleaned copy while this is on.",
            "inputSchema": {
                "type": "object",
                "properties": { "project": project, "on": { "type": "boolean", "default": true } },
                "required": ["project"],
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
        },
        {
            "name": "plan_zooms",
            "title": "Plan zooms again",
            "description": "Plans the automatic zooms again from the recorded clicks and typing, replacing the current ones.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "depth": { "type": "string", "enum": ["subtle", "standard", "bold"], "default": "standard" },
                },
                "required": ["project"],
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false },
        },
        {
            "name": "style_project",
            "title": "Style the canvas",
            "description": "Changes how the video looks: the wallpaper behind the window, the canvas shape, the space around the window, its corners and shadow, and the cursor's size.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "wallpaper": { "type": "string", "enum": wallpapers },
                    "aspect": { "type": "string", "enum": ["16:9", "4:3", "1:1", "4:5", "9:16", "original"] },
                    "padding": { "type": "number", "minimum": 0, "maximum": 0.3, "description": "Space around the window, as a fraction of the canvas's shorter side." },
                    "corner_radius": { "type": "number", "minimum": 0, "maximum": 60, "description": "In pixels at 1080p." },
                    "shadow": { "type": "number", "minimum": 0, "maximum": 1 },
                    "cursor_size": { "type": "number", "minimum": 0.5, "maximum": 4 },
                },
                "required": ["project"],
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false },
        },
        {
            "name": "prepare_take",
            "title": "Prepare a take",
            "description": "Opens an app (or brings it forward), finds its window and lists the controls in it, with ids to use as refs in a take script, plus a picture of the window. Plan the whole take from this, then call perform_take. Needs the Accessibility permission.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "app": { "type": "string", "description": "The app to film, e.g. \"Safari\", or its bundle id. Leave out to use the app in front." },
                    "window": { "type": "string", "description": "Part of the window's title, when the app has several." },
                    "picture": { "type": "boolean", "default": true, "description": "Include a picture of the window." },
                },
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
        },
        perform_take_tool(),
        {
            "name": "validate_take",
            "title": "Check a take script",
            "description": "Checks a take script (the beats an agent plans to perform) and lists any problems. The schema is in the Shotmaxxing docs and from `shotmaxxing take schema`.",
            "inputSchema": {
                "type": "object",
                "properties": { "script": { "type": ["object", "string"], "description": "The script, as JSON." } },
                "required": ["script"],
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false },
        },
    ])
}

fn text<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

fn project_arg(args: &Value) -> Result<PathBuf, String> {
    text(args, "project").map(expand).ok_or_else(|| "say which project: its .smx folder".to_string())
}

/// `~/…` the way people write it.
fn expand(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

fn open(path: &Path) -> Result<(ProjectBundle, Project), String> {
    ProjectBundle::open(path).map_err(|e| format!("couldn't open {}: {e}", path.display()))
}

fn call(
    state: &State,
    name: &str,
    args: &Value,
    progress: &dyn Fn(f64, &str),
    cancel: &Arc<AtomicBool>,
) -> Reply {
    match name {
        "list_sources" => shotmaxxing_capture::sources()
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::to_value(s).map_err(|e| e.to_string())),
        "list_devices" => Ok(json!({
            "microphones": shotmaxxing_capture::microphones(),
            "cameras": shotmaxxing_capture::cameras(),
        })),
        "start_recording" => start_recording(state, args),
        "recording_status" => Ok(recording_status(state)),
        "stop_recording" => stop_recording(state, args),
        "project_info" => {
            let (bundle, project) = open(&project_arg(args)?)?;
            Ok(summary(bundle.root(), &project))
        }
        "export_project" => export(args, progress, cancel),
        "cut_silences" => cut_silences(args),
        "clean_voice" => clean_voice(args, progress, cancel),
        "plan_zooms" => plan_zooms(args),
        "style_project" => style(args),
        "validate_take" => validate(args),
        "prepare_take" => prepare_take(state, args),
        "perform_take" => perform_take(state, args, progress, cancel),
        _ => Err(format!("there's no tool called {name}")),
    }
}

/// What a project holds, for agents to read.
pub fn summary(root: &Path, project: &Project) -> Value {
    let sec = |us: u64| (us as f64 / 1e4).round() / 100.0;
    let removed: u64 = project.edits.cuts.iter().map(|c| c.duration_us()).sum();
    json!({
        "project": root,
        "name": project.name,
        "recorded_seconds": sec(project.duration_us()),
        "seconds_after_cuts": sec(project.duration_us().saturating_sub(removed)),
        "size_points": { "width": project.source.size.width, "height": project.source.size.height },
        "pixels_per_point": project.source.scale,
        "app": project.source.app_name,
        "window": project.source.window_title,
        "tracks": project.tracks.iter().map(|t| json!({
            "kind": t.kind, "seconds": sec(t.duration_us), "offset_seconds": t.offset_us as f64 / 1e6, "volume": t.volume,
        })).collect::<Vec<_>>(),
        "zooms": project.zooms.iter().map(|z| json!({
            "start": sec(z.start_us), "end": sec(z.end_us), "scale": z.scale, "reason": z.reason,
        })).collect::<Vec<_>>(),
        "cuts": project.edits.cuts.iter().map(|c| json!({ "start": sec(c.start_us), "end": sec(c.end_us) })).collect::<Vec<_>>(),
        "voice": {
            "cleanup": project.voice.cleanup,
            "cut_silences": project.voice.cut_silences,
            "pauses_cut": project.voice.silences.len(),
        },
        "canvas": {
            "aspect": project.canvas.aspect,
            "wallpaper": match &project.canvas.background { Background::Wallpaper { id } => Some(id.clone()), _ => None },
            "padding": project.canvas.padding,
            "corner_radius": project.canvas.corner_radius,
        },
        "camera_bubble": project.webcam.visible,
    })
}

fn device(choice: Option<&str>) -> DeviceChoice {
    match choice {
        None => DeviceChoice::Off,
        Some("default") => DeviceChoice::Default,
        Some(id) => DeviceChoice::Id(id.to_string()),
    }
}

fn start_recording(state: &State, args: &Value) -> Reply {
    let mut slot = state.recording.lock().map_err(|_| "the recorder is busy".to_string())?;
    if slot.is_some() {
        return Err("a take is already recording; stop it first".into());
    }
    let target = if let Some(id) = args.get("window").and_then(Value::as_u64) {
        Target::Window(u32::try_from(id).map_err(|_| "that window id is too large")?)
    } else if let Some(app) = text(args, "app") {
        let needle = app.to_lowercase();
        let sources = shotmaxxing_capture::sources().map_err(|e| e.to_string())?;
        let window = sources
            .windows
            .iter()
            .filter(|w| w.app.to_lowercase().contains(&needle))
            .max_by(|a, b| (a.width * a.height).total_cmp(&(b.width * b.height)))
            .ok_or_else(|| format!("no open window belongs to “{app}”; list_sources shows them"))?;
        Target::Window(window.id)
    } else {
        Target::Display(args.get("display").and_then(Value::as_u64).and_then(|d| u32::try_from(d).ok()))
    };
    let mut options = ProjectOptions::new(target);
    options.max_fps = args.get("fps").and_then(Value::as_u64).map_or(60, |f| f.clamp(15, 120) as u32);
    options.hide_desktop_icons = args.get("hide_desktop_icons").and_then(Value::as_bool).unwrap_or(false);
    options.system_audio = args.get("system_audio").and_then(Value::as_bool).unwrap_or(false);
    options.microphone = device(text(args, "microphone"));
    options.camera = device(text(args, "camera"));
    let folder = text(args, "folder").map(expand).unwrap_or_else(crate::default_folder);
    let name = text(args, "name").unwrap_or("Recording").to_string();
    let limit =
        Duration::from_secs_f64(args.get("max_seconds").and_then(Value::as_f64).unwrap_or(900.0).max(1.0));

    let (ready_tx, ready_rx) = sync_channel::<Result<PathBuf, String>>(1);
    let (finish_tx, finish_rx) = channel::<bool>();
    let (done_tx, done_rx) = sync_channel(1);
    std::thread::Builder::new()
        .name("smx-mcp-recording".into())
        .spawn(move || {
            let recording = match ProjectRecording::start(&folder, &name, options) {
                Ok(recording) => recording,
                Err(error) => {
                    let _ = ready_tx.send(Err(error.to_string()));
                    return;
                }
            };
            let root = recording.root().to_path_buf();
            let _ = ready_tx.send(Ok(root.clone()));
            // Kept when told to stop, when the limit passes, or when the server goes away.
            let keep = finish_rx.recv_timeout(limit).unwrap_or(true);
            let result = recording.stop().map_err(|e| e.to_string());
            let result = if keep {
                result.map(Some)
            } else {
                let _ = std::fs::remove_dir_all(&root);
                Ok(None)
            };
            let _ = done_tx.send(result);
        })
        .map_err(|e| e.to_string())?;
    // The first take with a microphone or camera waits while the system asks the person.
    let root = match ready_rx.recv_timeout(Duration::from_secs(150)) {
        Ok(root) => root?,
        Err(_) => {
            let _ = finish_tx.send(false);
            return Err("the recording didn't start in time".into());
        }
    };
    *slot = Some(Session { root: root.clone(), started: Instant::now(), finish: finish_tx, done: done_rx });
    Ok(json!({ "recording": true, "project": root, "stops_by_itself_after_seconds": limit.as_secs_f64() }))
}

fn recording_status(state: &State) -> Value {
    match state.recording.lock().ok().as_deref() {
        Some(Some(session)) => json!({
            "recording": true,
            "project": session.root,
            "seconds": (session.started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
        }),
        _ => json!({ "recording": false }),
    }
}

fn stop_recording(state: &State, args: &Value) -> Reply {
    let session = state
        .recording
        .lock()
        .map_err(|_| "the recorder is busy".to_string())?
        .take()
        .ok_or_else(|| "no take is recording".to_string())?;
    let discard = args.get("discard").and_then(Value::as_bool).unwrap_or(false);
    match session.end(!discard)? {
        None => Ok(json!({ "discarded": true })),
        Some(finished) => {
            let mut report = summary(&finished.root, &finished.project);
            report["dropped_frames"] = json!(finished.dropped);
            report["warnings"] = json!(finished.warnings);
            Ok(report)
        }
    }
}

fn export(args: &Value, progress: &dyn Fn(f64, &str), cancel: &AtomicBool) -> Reply {
    let path = project_arg(args)?;
    let project = RenderProject::load(&path).map_err(|e| format!("couldn't open the project: {e}"))?;
    let format = match text(args, "format").unwrap_or("mp4") {
        "mp4" | "h264" => Format::H264,
        "hevc" | "h265" => Format::Hevc,
        "gif" => Format::Gif,
        other => return Err(format!("“{other}” isn't a format; use mp4, hevc or gif")),
    };
    let gif = format == Format::Gif;
    let size = args.get("size").and_then(Value::as_u64).map_or(if gif { 480 } else { 1080 }, |s| s as u32);
    let fps = args.get("fps").and_then(Value::as_u64).map_or(if gif { 15 } else { 60 }, |f| f as u32);
    let (width, height) = output_size(project.canvas_aspect(), size.clamp(240, 4320));
    let fps = fps.clamp(1, 120);
    let output = match text(args, "output") {
        Some(out) => expand(out),
        None => {
            let dir = path.parent().unwrap_or(Path::new("."));
            unique_path(dir, &sanitize_file_name(&project.name), if gif { "gif" } else { "mp4" })
        }
    };
    let options = ExportOptions {
        width,
        height,
        fps,
        format,
        bitrate: default_bitrate(width, height, fps, format),
        output: output.clone(),
    };
    let started = Instant::now();
    let mut last = -1.0;
    shotmaxxing_render::export(&path, &options, &mut |done| {
        // Two-percent steps keep the messages few.
        if done - last >= 0.02 || done >= 1.0 {
            last = done;
            progress(done, "Exporting");
        }
        !cancel.load(Ordering::SeqCst)
    })
    .map_err(|e| format!("the export didn't finish: {e}"))?;
    Ok(json!({
        "file": output,
        "width": width,
        "height": height,
        "fps": fps,
        "seconds": (project.output_duration() * 100.0).round() / 100.0,
        "took_seconds": (started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
    }))
}

/// Sorted, with overlapping and touching ranges joined.
fn tidy(mut ranges: Vec<TimeRange>) -> Vec<TimeRange> {
    ranges.sort_by_key(|r| r.start_us);
    let mut out: Vec<TimeRange> = Vec::with_capacity(ranges.len());
    for r in ranges {
        match out.last_mut() {
            Some(last) if r.start_us <= last.end_us => last.end_us = last.end_us.max(r.end_us),
            _ => out.push(r),
        }
    }
    out
}

fn cut_silences(args: &Value) -> Reply {
    let path = project_arg(args)?;
    let pauses =
        shotmaxxing_render::find_pauses(&path).map_err(|e| format!("couldn't look for pauses: {e}"))?;
    let (bundle, mut project) = open(&path)?;
    let us = |s: f64| (s * 1e6).round().max(0.0) as u64;
    let found: Vec<TimeRange> =
        pauses.iter().map(|&(a, b)| TimeRange { start_us: us(a), end_us: us(b) }).collect();
    if !found.is_empty() {
        bundle.snapshot(&project, "Before cutting silences").map_err(|e| e.to_string())?;
        let mut cuts = project.edits.cuts.clone();
        cuts.extend(found.iter().copied());
        project.edits.cuts = tidy(cuts);
        project.voice.cut_silences = true;
        project.voice.silences = tidy([project.voice.silences.clone(), found.clone()].concat());
        bundle.save(&mut project).map_err(|e| e.to_string())?;
    }
    let seconds: f64 = found.iter().map(|r| r.duration_us() as f64 / 1e6).sum();
    Ok(json!({
        "pauses_cut": found.len(),
        "seconds_cut": (seconds * 100.0).round() / 100.0,
        "note": if found.is_empty() { "No long pauses to cut: every pause over a second has something happening on screen, or there's no clear voice." } else { "Cut. The editor can bring them back." },
    }))
}

fn clean_voice(args: &Value, progress: &dyn Fn(f64, &str), cancel: &AtomicBool) -> Reply {
    let path = project_arg(args)?;
    let on = args.get("on").and_then(Value::as_bool).unwrap_or(true);
    let (bundle, mut project) = open(&path)?;
    let mut file = None;
    if on {
        let clean = bundle.root().join(shotmaxxing_render::CLEAN_MIC);
        if !clean.exists() {
            shotmaxxing_render::clean_voice(&path, &mut |done| {
                progress(done, "Cleaning up the voice");
                !cancel.load(Ordering::SeqCst)
            })
            .map_err(|e| format!("couldn't clean up the voice: {e}"))?;
        }
        file = Some(clean);
    }
    project.voice.cleanup = on;
    bundle.save(&mut project).map_err(|e| e.to_string())?;
    Ok(json!({ "cleanup": on, "file": file }))
}

fn plan_zooms(args: &Value) -> Reply {
    let path = project_arg(args)?;
    let depth = match text(args, "depth").unwrap_or("standard") {
        "subtle" => ZoomDepth::Subtle,
        "standard" => ZoomDepth::Standard,
        "bold" => ZoomDepth::Bold,
        other => return Err(format!("“{other}” isn't a depth; use subtle, standard or bold")),
    };
    let (root, project) = crate::replan(&path, depth).map_err(|e| format!("{e:#}"))?;
    Ok(summary(&root, &project)["zooms"].clone())
}

fn style(args: &Value) -> Reply {
    let path = project_arg(args)?;
    let (bundle, mut project) = open(&path)?;
    if let Some(id) = text(args, "wallpaper") {
        if !shotmaxxing_render::wallpaper::wallpapers().iter().any(|w| w.id == id) {
            return Err(format!("there's no wallpaper called “{id}”; tools/list shows them"));
        }
        project.canvas.background = Background::Wallpaper { id: id.to_string() };
    }
    if let Some(aspect) = text(args, "aspect") {
        project.canvas.aspect = serde_json::from_value::<AspectRatio>(json!(aspect))
            .map_err(|_| format!("“{aspect}” isn't a shape; use 16:9, 4:3, 1:1, 4:5, 9:16 or original"))?;
    }
    let number = |key: &str| args.get(key).and_then(Value::as_f64).filter(|v| v.is_finite());
    if let Some(padding) = number("padding") {
        project.canvas.padding = padding.clamp(0.0, 0.3);
    }
    if let Some(radius) = number("corner_radius") {
        project.canvas.corner_radius = radius.clamp(0.0, 60.0);
    }
    if let Some(shadow) = number("shadow") {
        // The editor's 0–1 shadow is the stored opacity over 0.6.
        project.canvas.shadow.opacity = shadow.clamp(0.0, 1.0) * 0.6;
    }
    if let Some(size) = number("cursor_size") {
        project.cursor.size = size.clamp(0.5, 4.0);
    }
    bundle.save(&mut project).map_err(|e| e.to_string())?;
    Ok(summary(bundle.root(), &project)["canvas"].clone())
}

fn parse_script(args: &Value) -> Result<TakeScript, String> {
    let script = args.get("script").ok_or_else(|| "pass the script".to_string())?;
    let text = match script {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    TakeScript::parse(&text).map_err(|e| format!("the script isn't a take script: {e}"))
}

fn validate(args: &Value) -> Reply {
    let script = parse_script(args)?;
    Ok(json!({ "valid": script.is_valid(), "beats": script.beats.len(), "issues": script.validate() }))
}

/// `perform_take`, with the take script's schema so agents can write scripts without guessing.
fn perform_take_tool() -> Value {
    let mut schema = shotmaxxing_core::take::json_schema();
    let defs = schema.as_object_mut().and_then(|s| s.remove("$defs")).unwrap_or_else(|| json!({}));
    if let Some(s) = schema.as_object_mut() {
        s.remove("$schema");
    }
    let mut defs = defs.as_object().cloned().unwrap_or_default();
    defs.insert("TakeScript".into(), schema);
    json!({
        "name": "perform_take",
        "title": "Perform and film a take",
        "description": "Films a take script. Shotmaxxing records the app's window while it moves the pointer, clicks and types at a human pace, then saves an editable project, and exports a video when the script's output mode is video. Targets are refs from prepare_take (checked again just before each click), screen points, or { find: label } for controls that appear later. The person can stop the take at any time by pressing Esc three times or moving the mouse. user_step beats hand control to them until they press ⌥⌘Return, and that stretch is cut from the video. It never types into password fields. Use dry_run to see the timeline first. Needs the Screen Recording and Accessibility permissions.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "script": { "anyOf": [{ "$ref": "#/$defs/TakeScript" }, { "type": "string", "description": "The script as YAML." }] },
                "dry_run": { "type": "boolean", "default": false, "description": "Work out the timeline without moving anything." },
                "folder": { "type": "string", "description": "Where the project goes. Defaults to Movies/Shotmaxxing." },
                "microphone": { "type": "string", "description": "A microphone id from list_devices, or \"default\", to record narration." },
                "system_audio": { "type": "boolean", "default": false, "description": "Record what the computer plays." },
            },
            "required": ["script"],
            "$defs": defs,
        },
        "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": true },
    })
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

fn prepare_take(state: &State, args: &Value) -> Reply {
    let prepared =
        shotmaxxing_agent::prepare(text(args, "app"), text(args, "window")).map_err(|e| e.to_string())?;
    const LISTED: usize = 300;
    let round = |v: f64| v.round();
    let controls: Vec<Value> = prepared
        .controls
        .iter()
        .take(LISTED)
        .map(|c| {
            json!({
                "id": c.id, "role": c.role, "label": c.label, "enabled": c.enabled,
                "x": round(c.bounds.x), "y": round(c.bounds.y), "width": round(c.bounds.width), "height": round(c.bounds.height),
            })
        })
        .collect();
    let mut value = json!({
        "app": prepared.app,
        "window": prepared.window.as_ref().map(|w| json!({
            "id": w.id, "title": w.title, "x": w.x, "y": w.y, "width": w.width, "height": w.height,
        })),
        "controls": controls,
        "more_controls": prepared.truncated || prepared.controls.len() > LISTED,
        "how_to_use": "Name controls by id as refs, e.g. click: button.export. Points are global screen points. A control that only appears later is found by its label with { find: \"Export\" }.",
    });
    if args.get("picture").and_then(Value::as_bool) != Some(false)
        && let Some(window) = &prepared.window
        && let Ok(pictures) = shotmaxxing_capture::thumbnails(&[Target::Window(window.id)], 1280)
        && let Some(Some(jpeg)) = pictures.into_iter().next()
    {
        if let Some((width, _)) = jpeg_size(&jpeg) {
            let per_pixel = window.width / f64::from(width.max(1));
            value["picture"] = json!({
                "origin": { "x": window.x, "y": window.y },
                "points_per_pixel": (per_pixel * 1000.0).round() / 1000.0,
                "note": "A pixel (px, py) in the picture is the screen point (origin.x + px × points_per_pixel, origin.y + py × points_per_pixel).",
            });
        }
        value["_image"] = json!({ "type": "image", "data": base64(&jpeg), "mimeType": "image/jpeg" });
    }
    if let Ok(mut slot) = state.prepared.lock() {
        *slot = Some(prepared);
    }
    Ok(value)
}

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

/// The export a script's preset asks for: format, shorter side, frame rate and canvas shape.
fn preset_export(preset: Option<ExportPreset>) -> (&'static str, u32, u32, Option<&'static str>) {
    match preset.unwrap_or_default() {
        ExportPreset::Youtube1080 | ExportPreset::Custom => ("mp4", 1080, 60, None),
        ExportPreset::Social => ("mp4", 1080, 30, Some("1:1")),
        ExportPreset::Vertical => ("mp4", 1080, 30, Some("9:16")),
        ExportPreset::Gif => ("gif", 480, 15, None),
        ExportPreset::Uhd4k => ("hevc", 2160, 60, None),
    }
}

fn perform_take(
    state: &State,
    args: &Value,
    progress: &dyn Fn(f64, &str),
    cancel: &Arc<AtomicBool>,
) -> Reply {
    let script = parse_script(args)?;
    let planned = state
        .prepared
        .lock()
        .ok()
        .and_then(|p| p.clone())
        .filter(|p| {
            script.app.as_deref().is_none_or(|app| {
                let (app, seen) = (app.to_lowercase(), p.app.to_lowercase());
                seen.contains(&app) || p.window.as_ref().is_some_and(|w| w.bundle_id.to_lowercase() == app)
            })
        })
        .map(|p| p.controls)
        .unwrap_or_default();
    if args.get("dry_run").and_then(Value::as_bool).unwrap_or(false) {
        return serde_json::to_value(shotmaxxing_agent::plan(&script, &planned)).map_err(|e| e.to_string());
    }
    if state.recording.lock().is_ok_and(|r| r.is_some()) {
        return Err("a recording is running; stop it before filming a take".into());
    }
    if state.taking.swap(true, Ordering::SeqCst) {
        return Err("a take is already being filmed".into());
    }
    let _taking = Taking(state.taking.clone());
    let options = shotmaxxing_agent::TakeOptions {
        folder: text(args, "folder").map(expand).unwrap_or_else(crate::default_folder),
        planned,
        microphone: device(text(args, "microphone")),
        system_audio: args.get("system_audio").and_then(Value::as_bool).unwrap_or(false),
        client: state.client.lock().ok().and_then(|c| c.clone()),
        cancel: cancel.clone(),
    };
    let take = shotmaxxing_agent::run_take(&script, options, &mut |p| progress(p.done * 0.9, &p.message))
        .map_err(|e| e.to_string())?;
    let root = take.finished.root.clone();
    let mut report = summary(&root, &take.finished.project);
    let mut warnings = take.finished.warnings.clone();
    warnings.extend(take.performed.warnings.iter().cloned());
    report["performance"] = json!({
        "beats": take.performed.beats,
        "beats_done": take.performed.beats_done,
        "stopped": take.performed.stopped,
        "captions": take.performed.captions.len(),
        "handed_over": take.performed.handoffs.len(),
    });
    report["warnings"] = json!(warnings);
    if take.performed.stopped.is_none() && script.output.mode == OutputMode::Video {
        let (format, size, fps, aspect) = preset_export(script.output.preset);
        if let Some(aspect) = aspect {
            style(&json!({ "project": root, "aspect": aspect }))?;
        }
        let output = script.output.label.as_deref().map(|label| {
            let name = sanitize_file_name(&format!("{} {label}", take.finished.project.name));
            unique_path(
                root.parent().unwrap_or(Path::new(".")),
                &name,
                if format == "gif" { "gif" } else { "mp4" },
            )
        });
        let export_args =
            json!({ "project": root, "format": format, "size": size, "fps": fps, "output": output });
        report["video"] = export(&export_args, &|done, message| progress(0.9 + done * 0.1, message), cancel)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_base64_and_reads_jpeg_sizes() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        // SOI, an APP0 segment, then a baseline frame header for 640×360.
        let jpeg = [
            0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x01, 0x68, 0x02,
            0x80, 0x03,
        ];
        assert_eq!(jpeg_size(&jpeg), Some((640, 360)));
        assert_eq!(jpeg_size(b"not a jpeg at all"), None);
    }

    #[test]
    fn ranges_are_sorted_and_joined() {
        let r = |a, b| TimeRange { start_us: a, end_us: b };
        assert_eq!(tidy(vec![r(5, 8), r(0, 2), r(1, 3), r(8, 9)]), vec![r(0, 3), r(5, 9)]);
    }

    #[test]
    fn every_tool_has_a_schema_and_a_description() {
        let tools = tools();
        let tools = tools.as_array().unwrap();
        assert!(tools.len() >= 12);
        for tool in tools {
            assert!(tool["description"].as_str().is_some_and(|d| d.len() > 20), "{tool}");
            assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
        }
    }
}
