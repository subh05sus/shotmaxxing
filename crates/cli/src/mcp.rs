//! `shotmaxxing mcp`: Shotmaxxing as an MCP server for AI agents, over stdin and stdout.
//!
//! Agents such as Claude Code, Codex and Cursor start it and call its tools: see what can be recorded, record a
//! take, read what came out, tidy it up (cut silences, clean up the voice, plan zooms, style the canvas) and export
//! it. Messages are JSON-RPC 2.0, one per line. Every tool call runs on a thread of its own, so the server keeps
//! answering (and can be told to cancel) while a take records or an export renders. Logs go to stderr; stdout
//! carries nothing but protocol messages.
//!
//! Whatever takes the system's permissions (seeing the screen, recording it, moving the pointer and typing) the
//! Shotmaxxing app does, over the control API, after the person allows it there; this server opens the app when it
//! isn't running. So the agent's terminal needs no permissions. Projects and exports are files, worked on here.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use shotmaxxing_control::{Client, FromApp, RecordRequest, TakeRequest, TakeResult, ToApp};
use shotmaxxing_core::bundle::{ProjectBundle, sanitize_file_name};
use shotmaxxing_core::project::ExportPreset;
use shotmaxxing_core::project::{
    AspectRatio, Background, MediaTrack, Project, TimeRange, TrackKind, ZoomDepth,
};
use shotmaxxing_core::take::{OutputMode, TakeScript};
use shotmaxxing_render::{
    ExportOptions, Format, RenderProject, default_bitrate, output_size, png_size, unique_path,
};

/// Protocol versions this server speaks, newest first.
const PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Shotmaxxing records the screen into editable projects (.smx folders) and exports \
polished videos: a wallpaper behind the window, smooth automatic zooms on clicks and typing, a clean cursor, and \
the voice and camera when recorded. The best demos are planned takes: prepare_take lists an app's controls and \
shows its window, then perform_take films a script while Shotmaxxing moves the pointer and types at a human \
pace (try dry_run first). The Shotmaxxing app does the looking, recording and filming with its own permissions, \
so this server needs none, and it opens the app when it isn't running. The person allows it there: once per \
session before Shotmaxxing shows you the screen or records it, and before every planned take, which then plays \
live on the app's take card. To film something else doing the work: list_sources, start_recording, the demo, \
stop_recording. Then export_project. project_info shows what a project holds; cut_silences, clean_voice, \
plan_zooms, style_project and set_music (only with a music file the person chose) tidy it up before exporting.";

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

#[derive(Clone, Default)]
struct State {
    /// The project of the recording this session started in the app, while it may still be running.
    recording: Arc<Mutex<Option<PathBuf>>>,
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

/// Runs the server until the client closes stdin. A recording this session left running is then stopped and kept.
pub fn serve() -> anyhow::Result<()> {
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
    let recording = state.recording.lock().ok().and_then(|mut slot| slot.take());
    if recording.is_some()
        && let Err(error) =
            ask_app(&state, ToApp::StopRecording { discard: false }, &AtomicBool::new(false), Launch::No)
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
    // The built-in wallpapers, then those of the content packs added in the app (`<pack>.<wallpaper>`).
    let wallpapers: Vec<String> =
        shotmaxxing_render::wallpaper::wallpapers()
            .into_iter()
            .map(|w| w.id)
            .chain(shotmaxxing_render::packs::installed().iter().flat_map(|pack| {
                pack.wallpapers.iter().map(|w| pack.wallpaper_id(&w.id)).collect::<Vec<_>>()
            }))
            .collect();
    json!([
        {
            "name": "list_sources",
            "title": "List screens and windows",
            "description": "The screens and windows that can be recorded, with their ids and sizes in points. The first time a session uses the screen, the person allows it in the Shotmaxxing app (the call waits up to two minutes).",
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
            "description": "Starts recording a screen or a window into a new project, with clicks, typing and the cursor path for automatic zooms. The real cursor is left out of the video and drawn cleanly later. The Shotmaxxing app records it and shows its recording pill, where the person can stop it; the first time a session uses the screen, they allow it there. One recording runs at a time; it stops by itself after max_seconds.",
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
            "description": "Whether the recording this session started is still running, which project it goes into and for how long it has run.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": { "readOnlyHint": true, "openWorldHint": false },
        },
        {
            "name": "stop_recording",
            "title": "Stop recording",
            "description": "Stops the recording this session started, plans automatic zooms and saves the project. Returns what it holds: length, zooms, tracks, and anything that went wrong, such as a microphone that wasn't allowed. If the person already stopped it in Shotmaxxing, or it reached max_seconds, this reports the saved project.",
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
            "description": "Renders the project with every edit applied, on the GPU: MP4 (H.264) plays everywhere, HEVC is smaller, GIF loops in chats and docs, and PNG saves one frame (the moment given by at) styled like the video, for docs and posts. A screenshot project (kind screenshot) exports only as PNG, at its own pixels unless a size is given. Reports progress and can be cancelled.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "format": { "type": "string", "enum": ["mp4", "hevc", "gif", "png"], "description": "Defaults to mp4, or png for a screenshot." },
                    "at": { "type": "number", "minimum": 0, "description": "For png: the moment to show, in seconds into the recording, as zoom times are given. Defaults to the start." },
                    "size": { "type": "integer", "minimum": 240, "maximum": 4320, "description": "The shorter side in pixels. Defaults to 1080, 480 for a GIF, or a screenshot's own pixels." },
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
            "name": "set_music",
            "title": "Set the music",
            "description": "Puts music under the whole video. It plays on the finished video's timeline from its start (or `start` seconds in), straight through cuts and speed changes, and fades out at the end. The file is copied into the project. Give only `volume` or `start` to change the music that's there, or `remove: true` to take it out.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project": project,
                    "file": { "type": "string", "description": "An MP3, M4A, AAC, WAV, AIFF, CAF or FLAC file." },
                    "volume": { "type": "number", "minimum": 0, "maximum": 1.5, "description": "1 is as loud as the file. Defaults to 0.3 under a voice and 0.8 without one." },
                    "start": { "type": "number", "minimum": 0, "description": "Seconds into the finished video where the music starts. Defaults to 0." },
                    "remove": { "type": "boolean", "default": false },
                },
                "required": ["project"],
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false },
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
            "description": "Changes how the video looks: the wallpaper behind the window (the built-in ones, then those of content packs added in the app), the canvas shape, the space around the window, its corners and shadow, and the cursor's size.",
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
            "description": "Opens an app (or brings it forward), finds its window and lists the controls in it, with ids to use as refs in a take script, plus a picture of the window. Plan the whole take from this, then call perform_take. The Shotmaxxing app does this with its own permissions; the first time a session uses the screen, the person allows it there.",
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
        "list_sources" => match ask_app(state, ToApp::Sources, cancel, Launch::Yes)? {
            FromApp::Sources { sources } => Ok(sources),
            other => Err(unexpected(&other)),
        },
        "list_devices" => Ok(json!({
            "microphones": shotmaxxing_capture::microphones(),
            "cameras": shotmaxxing_capture::cameras(),
        })),
        "start_recording" => start_recording(state, args, cancel),
        "recording_status" => recording_status(state, cancel),
        "stop_recording" => stop_recording(state, args, cancel),
        "project_info" => {
            let (bundle, project) = open(&project_arg(args)?)?;
            Ok(summary(bundle.root(), &project))
        }
        "export_project" => export(args, progress, cancel),
        "cut_silences" => cut_silences(args),
        "clean_voice" => clean_voice(args, progress, cancel),
        "plan_zooms" => plan_zooms(args),
        "set_music" => set_music(args),
        "style_project" => style(args),
        "validate_take" => validate(args),
        "prepare_take" => prepare_take(state, args, cancel),
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
        "kind": project.kind,
        "recorded_seconds": sec(project.duration_us()),
        "seconds_after_cuts": sec(project.duration_us().saturating_sub(removed)),
        "size_points": { "width": project.source.size.width, "height": project.source.size.height },
        "pixels_per_point": project.source.scale,
        "app": project.source.app_name,
        "window": project.source.window_title,
        "tracks": project.tracks.iter().map(|t| json!({
            "kind": t.kind, "name": t.name, "seconds": sec(t.duration_us), "offset_seconds": t.offset_us as f64 / 1e6, "volume": t.volume,
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
            "wallpaper": match &project.canvas.background { Background::Wallpaper { id, .. } => Some(id.clone()), _ => None },
            "padding": project.canvas.padding,
            "corner_radius": project.canvas.corner_radius,
        },
        "camera_bubble": project.webcam.visible,
    })
}

fn start_recording(state: &State, args: &Value, cancel: &AtomicBool) -> Reply {
    let window = match args.get("window").and_then(Value::as_u64) {
        Some(id) => Some(u32::try_from(id).map_err(|_| "that window id is too large")?),
        None => None,
    };
    let record = RecordRequest {
        window,
        app: text(args, "app").map(str::to_string),
        display: args.get("display").and_then(Value::as_u64).and_then(|d| u32::try_from(d).ok()),
        name: text(args, "name").unwrap_or("Recording").to_string(),
        folder: text(args, "folder").map(expand).unwrap_or_else(crate::default_folder),
        fps: args.get("fps").and_then(Value::as_u64).map_or(60, |f| f.clamp(15, 120) as u32),
        hide_desktop_icons: args.get("hide_desktop_icons").and_then(Value::as_bool).unwrap_or(false),
        system_audio: args.get("system_audio").and_then(Value::as_bool).unwrap_or(false),
        microphone: text(args, "microphone").map(str::to_string),
        camera: text(args, "camera").map(str::to_string),
        max_seconds: args.get("max_seconds").and_then(Value::as_f64).unwrap_or(900.0).max(1.0),
    };
    match ask_app(state, ToApp::Record { record }, cancel, Launch::Yes)? {
        FromApp::Recording { status: Some(status) } => {
            if let Ok(mut slot) = state.recording.lock() {
                *slot = Some(status.project.clone());
            }
            Ok(json!({
                "recording": true,
                "project": status.project,
                "stops_by_itself_after_seconds": status.stops_after_seconds,
            }))
        }
        other => Err(unexpected(&other)),
    }
}

fn recording_status(state: &State, cancel: &AtomicBool) -> Reply {
    let Some(root) = state.recording.lock().ok().and_then(|slot| slot.clone()) else {
        return Ok(json!({ "recording": false }));
    };
    match ask_app(state, ToApp::RecordingStatus, cancel, Launch::No) {
        Ok(FromApp::Recording { status: Some(status) }) => Ok(json!({
            "recording": true,
            "project": status.project,
            "seconds": status.seconds,
            "stops_by_itself_after_seconds": status.stops_after_seconds,
        })),
        Ok(FromApp::Recording { status: None }) | Err(_) => Ok(json!({
            "recording": false,
            "project": root,
            "note": "It has stopped: the person stopped it in Shotmaxxing, it reached max_seconds, or the app quit. stop_recording reports what it holds.",
        })),
        Ok(other) => Err(unexpected(&other)),
    }
}

fn stop_recording(state: &State, args: &Value, cancel: &AtomicBool) -> Reply {
    let root = state
        .recording
        .lock()
        .map_err(|_| "the recorder is busy".to_string())?
        .take()
        .ok_or_else(|| "no recording from this session is running".to_string())?;
    let discard = args.get("discard").and_then(Value::as_bool).unwrap_or(false);
    let report = |root: &Path| -> Reply {
        let (_, project) = open(root)?;
        Ok(summary(root, &project))
    };
    match ask_app(state, ToApp::StopRecording { discard }, cancel, Launch::No) {
        Ok(FromApp::Recorded { recorded }) => match recorded.project {
            None => Ok(json!({ "discarded": true })),
            Some(project) => {
                let mut summary = report(&project)?;
                summary["dropped_frames"] = json!(recorded.dropped_frames);
                summary["warnings"] = json!(recorded.warnings);
                Ok(summary)
            }
        },
        // It ended without this session: the person stopped or threw it away in the app, it reached its limit, or
        // the app quit (which keeps it).
        Ok(FromApp::Recording { status: None }) | Err(_) if root.join("project.json").is_file() => {
            let mut summary = report(&root)?;
            summary["note"] = json!(if discard {
                "It had already stopped and been saved in Shotmaxxing, so it wasn't thrown away."
            } else {
                "It had already stopped: the person stopped it in Shotmaxxing, or it reached max_seconds."
            });
            Ok(summary)
        }
        Ok(FromApp::Recording { status: None }) => Ok(json!({
            "discarded": true,
            "note": "The recording was thrown away in Shotmaxxing.",
        })),
        Ok(other) => Err(unexpected(&other)),
        Err(error) => Err(error),
    }
}

fn export(args: &Value, progress: &dyn Fn(f64, &str), cancel: &AtomicBool) -> Reply {
    let path = project_arg(args)?;
    let project = RenderProject::load(&path).map_err(|e| format!("couldn't open the project: {e}"))?;
    let format = match text(args, "format").unwrap_or(if project.still { "png" } else { "mp4" }) {
        "mp4" | "h264" => Format::H264,
        "hevc" | "h265" => Format::Hevc,
        "gif" => Format::Gif,
        "png" => Format::Png,
        other => return Err(format!("“{other}” isn't a format; use mp4, hevc, gif or png")),
    };
    let gif = format == Format::Gif;
    let size = args.get("size").and_then(Value::as_u64).map_or(if gif { 480 } else { 1080 }, |s| s as u32);
    let fps = args.get("fps").and_then(Value::as_u64).map_or(if gif { 15 } else { 60 }, |f| f as u32);
    // A screenshot comes out pixel for pixel (0 × 0) unless a size is asked for.
    let native = project.still && args.get("size").is_none();
    let (width, height) =
        if native { (0, 0) } else { output_size(project.canvas_aspect(), size.clamp(240, 4320)) };
    let fps = fps.clamp(1, 120);
    let output = match text(args, "output") {
        Some(out) => expand(out),
        None => {
            let dir = path.parent().unwrap_or(Path::new("."));
            unique_path(dir, &sanitize_file_name(&project.name), format.extension())
        }
    };
    let options = ExportOptions {
        width,
        height,
        fps,
        format,
        bitrate: default_bitrate(width, height, fps, format),
        output: output.clone(),
        at: (format == Format::Png).then(|| args.get("at").and_then(Value::as_f64).unwrap_or(0.0)),
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
    let (width, height) = if native { png_size(&output).unwrap_or_default() } else { (width, height) };
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

fn set_music(args: &Value) -> Reply {
    let path = project_arg(args)?;
    let (bundle, mut project) = open(&path)?;
    let current = project.tracks.iter().find(|t| t.kind == TrackKind::Music).cloned();
    if args.get("remove").and_then(Value::as_bool).unwrap_or(false) {
        project.tracks.retain(|t| t.kind != TrackKind::Music);
        bundle.save(&mut project).map_err(|e| e.to_string())?;
        let note = if current.is_some() { "Took the music out." } else { "There was no music to take out." };
        return Ok(json!({ "music": null, "note": note }));
    }
    let mut track = match (text(args, "file"), current) {
        (Some(file), current) => {
            let music = shotmaxxing_render::import_music(bundle.root(), &expand(file))
                .map_err(|e| format!("couldn't add the music: {e}"))?;
            let voice = project.tracks.iter().any(|t| t.kind == TrackKind::Microphone && !t.muted);
            MediaTrack {
                id: "music".into(),
                kind: TrackKind::Music,
                path: music.path,
                duration_us: (music.seconds * 1e6).round() as u64,
                // New music keeps the old music's place and level.
                offset_us: current.as_ref().map_or(0, |t| t.offset_us),
                volume: current.as_ref().map_or(if voice { 0.3 } else { 0.8 }, |t| t.volume),
                muted: false,
                name: Some(music.name),
            }
        }
        (None, Some(current)) => current,
        (None, None) => return Err("give a music file: this project has no music yet".into()),
    };
    if let Some(volume) = args.get("volume").and_then(Value::as_f64) {
        track.volume = volume.clamp(0.0, 1.5);
    }
    if let Some(start) = args.get("start").and_then(Value::as_f64) {
        track.offset_us = (start.max(0.0) * 1e6).round() as i64;
    }
    project.tracks.retain(|t| t.kind != TrackKind::Music);
    project.tracks.push(track.clone());
    bundle.save(&mut project).map_err(|e| e.to_string())?;
    let seconds = track.duration_us as f64 / 1e6;
    let start = track.offset_us as f64 / 1e6;
    Ok(json!({
        "music": {
            "name": track.name,
            "file": track.path,
            "seconds": (seconds * 100.0).round() / 100.0,
            "volume": track.volume,
            "start": start,
        },
        "note": "It plays under the export from the start you gave, through cuts, and fades out at the end.",
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

/// What a project keeps of the wallpaper `id`: nothing for a built-in one, a copy of a content pack's.
fn wallpaper_css(id: &str) -> Result<Option<String>, String> {
    if shotmaxxing_render::wallpaper::wallpapers().iter().any(|w| w.id == id) {
        return Ok(None);
    }
    shotmaxxing_render::packs::installed()
        .into_iter()
        .find_map(|pack| {
            let own = pack.wallpapers.iter().find(|w| pack.wallpaper_id(&w.id) == id)?;
            Some(own.css.clone())
        })
        .map(Some)
        .ok_or_else(|| format!("there's no wallpaper called “{id}”; tools/list shows them"))
}

fn style(args: &Value) -> Reply {
    let path = project_arg(args)?;
    let (bundle, mut project) = open(&path)?;
    if let Some(id) = text(args, "wallpaper") {
        project.canvas.background = Background::Wallpaper { id: id.to_string(), css: wallpaper_css(id)? };
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
        "description": "Films a take script. Shotmaxxing records the app's window while it moves the pointer, clicks and types at a human pace, then saves an editable project, and exports a video when the script's output mode is video. Targets are refs from prepare_take (checked again just before each click), screen points, or { find: label } for controls that appear later. The Shotmaxxing app films it with its own permissions, opening first if it isn't running: the person allows every take on its take card (the call waits up to two minutes for their answer), then watches it live there. The person can stop the take at any time with the card's Stop button, by pressing Esc three times, or by moving the mouse, and can pause it or take over from the card between steps (that time is cut from the video, and counts in performance.handed_over). user_step beats hand control to them until they press ⌥⌘Return or Hand back on the card, and that stretch is cut from the video. It never types into password fields. Use dry_run to see the timeline first.",
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

fn prepare_take(state: &State, args: &Value, cancel: &AtomicBool) -> Reply {
    let request = ToApp::Prepare {
        app: text(args, "app").map(str::to_string),
        window: text(args, "window").map(str::to_string),
        picture: args.get("picture").and_then(Value::as_bool) != Some(false),
    };
    let (prepared, picture) = match ask_app(state, request, cancel, Launch::Yes)? {
        FromApp::Prepared { prepared, picture } => (prepared, picture),
        other => return Err(unexpected(&other)),
    };
    let prepared: shotmaxxing_agent::Prepared = serde_json::from_value(prepared)
        .map_err(|e| format!("the Shotmaxxing app's answer didn't make sense: {e}"))?;
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
    if let (Some(picture), Some(window)) = (picture, &prepared.window) {
        if picture.width > 0 {
            let per_pixel = window.width / f64::from(picture.width);
            value["picture"] = json!({
                "origin": { "x": window.x, "y": window.y },
                "points_per_pixel": (per_pixel * 1000.0).round() / 1000.0,
                "note": "A pixel (px, py) in the picture is the screen point (origin.x + px × points_per_pixel, origin.y + py × points_per_pixel).",
            });
        }
        value["_image"] = json!({ "type": "image", "data": picture.data, "mimeType": "image/jpeg" });
    }
    if let Ok(mut slot) = state.prepared.lock() {
        *slot = Some(prepared);
    }
    Ok(value)
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

/// How long the person has to answer in the app before a take is called off.
const ASK_SECONDS: u64 = 120;

/// How long a request may take in the app beyond the person's answer: starting a recording can wait on the system's
/// microphone and camera questions.
const WORK_SECONDS: u64 = 180;

/// How long the app may take to open and start listening.
const OPEN_SECONDS: u64 = 30;

/// This agent session, as the app knows it: one `shotmaxxing mcp` process.
fn session_id() -> &'static str {
    static SESSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SESSION.get_or_init(|| {
        let nanos =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        format!("{}-{nanos}", std::process::id())
    })
}

/// Whether reaching the app may open it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Launch {
    Yes,
    /// Only a running app will do: there's nothing to ask a closed one.
    No,
}

/// A connection to the app for one request, after the hello. Opens the app first when it isn't running and
/// `launch` allows it, except when `SHOTMAXXING_CONTROL_SOCKET` points somewhere else (tests and development): then
/// only an app already listening there will do.
fn reach_app(state: &State, launch: Launch) -> Result<Client, String> {
    let name = state.client.lock().ok().and_then(|c| c.clone()).unwrap_or_else(|| "An agent".into());
    let elsewhere = std::env::var_os("SHOTMAXXING_CONTROL_SOCKET").is_some_and(|p| !p.is_empty());
    let mut client = match Client::connect() {
        Some(client) => client,
        None if launch == Launch::Yes && !elsewhere => open_app()?,
        None => return Err("Shotmaxxing isn't open. Open the Shotmaxxing app, then try again.".into()),
    };
    client.hello(&name, session_id())?;
    Ok(client)
}

/// Opens a saved project in the app's editor, starting the app first when it isn't running. For `shotmaxxing open`,
/// and for takes run in a terminal whose script asks for the editor.
pub fn open_in_editor(project: &Path) -> Result<(), String> {
    let project =
        std::path::absolute(project).map_err(|e| format!("couldn't find {}: {e}", project.display()))?;
    let state = State::default();
    if let Ok(mut client) = state.client.lock() {
        *client = Some("Shotmaxxing CLI".into());
    }
    match ask_app(&state, ToApp::Open { project }, &AtomicBool::new(false), Launch::Yes)? {
        FromApp::Opened => Ok(()),
        other => Err(unexpected(&other)),
    }
}

/// Opens the app and waits until it listens.
fn open_app() -> Result<Client, String> {
    start_app()?;
    let deadline = Instant::now() + Duration::from_secs(OPEN_SECONDS);
    while Instant::now() < deadline {
        if let Some(client) = Client::connect() {
            return Ok(client);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err("Shotmaxxing didn't open in time. Open the Shotmaxxing app, then try again.".into())
}

/// Starts the app this command came with: the bundle it sits in, or a development build beside it; otherwise the
/// installed app. It opens behind the agent's window.
#[cfg(target_os = "macos")]
fn start_app() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("couldn't find the Shotmaxxing app: {e}"))?;
    let bundle = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app"));
    let beside = exe.with_file_name("shotmaxxing-desktop");
    if bundle.is_none() && beside.is_file() {
        return spawn_app(&beside);
    }
    let mut open = std::process::Command::new("/usr/bin/open");
    open.arg("-g");
    match bundle {
        Some(bundle) => open.arg(bundle),
        None => open.args(["-b", shotmaxxing_core::APP_ID]),
    };
    let status = open
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("couldn't open Shotmaxxing: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("Shotmaxxing isn't installed, or macOS couldn't open it. Open the Shotmaxxing app, then try again.".into())
    }
}

/// Starts the app installed beside this command, or a development build beside it.
#[cfg(windows)]
fn start_app() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("couldn't find the Shotmaxxing app: {e}"))?;
    ["Shotmaxxing.exe", "shotmaxxing-desktop.exe"]
        .into_iter()
        .map(|name| exe.with_file_name(name))
        .find(|path| path.is_file())
        .ok_or_else(|| {
            "Shotmaxxing isn't installed beside this command. Open the Shotmaxxing app, then try again."
                .to_string()
        })
        .and_then(|app| spawn_app(&app))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn start_app() -> Result<(), String> {
    Err("Open the Shotmaxxing app, then try again.".into())
}

/// Starts the app as a process of its own, so it carries on when this command (or the agent's terminal) ends.
#[cfg(any(target_os = "macos", windows))]
fn spawn_app(program: &Path) -> Result<(), String> {
    let mut command = std::process::Command::new(program);
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    let mut child = command.spawn().map_err(|e| format!("couldn't open Shotmaxxing: {e}"))?;
    // Collected when it quits, so it doesn't linger while this command runs.
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// Sends one request to the app and waits for its answer: as long as the person may take to answer on the card and
/// the work needs, or until the call is cancelled (which also takes the question off the card).
fn ask_app(state: &State, request: ToApp, cancel: &AtomicBool, launch: Launch) -> Result<FromApp, String> {
    let mut client = reach_app(state, launch)?;
    client.send(&request).map_err(|e| format!("lost touch with Shotmaxxing: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(ASK_SECONDS + WORK_SECONDS);
    match client.receive_until(deadline, cancel) {
        Ok(Some(FromApp::Failed { message })) => Err(message),
        Ok(Some(reply)) => Ok(reply),
        Ok(None) => Err("the Shotmaxxing app closed the connection before answering".into()),
        Err(_) if cancel.load(Ordering::SeqCst) => Err("cancelled".into()),
        Err(error) => Err(format!("the Shotmaxxing app didn't answer in time: {error}")),
    }
}

fn unexpected(reply: &FromApp) -> String {
    let kind = serde_json::to_value(reply).ok().and_then(|v| v["type"].as_str().map(str::to_string));
    format!("the Shotmaxxing app gave an answer this command doesn't expect ({})", kind.unwrap_or_default())
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
    if state.taking.swap(true, Ordering::SeqCst) {
        return Err("a take is already being filmed".into());
    }
    let _taking = Taking(state.taking.clone());

    let plan = shotmaxxing_agent::plan(&script, &planned);
    let take = TakeRequest {
        script: script.clone(),
        planned,
        beats: plan.beats.iter().map(|b| b.what.clone()).collect(),
        seconds: plan.seconds,
        folder: text(args, "folder").map(expand).unwrap_or_else(crate::default_folder),
        microphone: text(args, "microphone").map(str::to_string),
        system_audio: args.get("system_audio").and_then(Value::as_bool).unwrap_or(false),
    };
    progress(0.0, "Asking in Shotmaxxing");
    let mut client = reach_app(state, Launch::Yes)?;
    client
        .send(&ToApp::Take { take: Box::new(take) })
        .map_err(|e| format!("lost touch with Shotmaxxing: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(ASK_SECONDS + 10);
    match client.receive_until(deadline, cancel) {
        Ok(Some(FromApp::Answer { allowed: true, .. })) => {}
        Ok(Some(FromApp::Answer { allowed: false, reason })) => {
            return Err(reason.unwrap_or_else(|| "The person said no to this take in Shotmaxxing.".into()));
        }
        Ok(Some(FromApp::Failed { message })) => return Err(message),
        Ok(Some(_) | None) => return Err("Shotmaxxing closed before the person answered.".into()),
        Err(_) if cancel.load(Ordering::SeqCst) => return Err("cancelled".into()),
        Err(_) => return Err("Nobody answered in Shotmaxxing, so the take didn't start.".into()),
    }

    // The take plays in the app, and its progress comes here. Calling the tool off stops it there at once; what was
    // filmed so far is kept.
    let mut sender = client.sender().map_err(|e| format!("lost touch with Shotmaxxing: {e}"))?;
    let never = AtomicBool::new(false);
    let mut called_off = false;
    let performed = loop {
        // A take can wait for the person as long as they need, so only the agent or the app ends this.
        let deadline = Instant::now() + Duration::from_secs(24 * 60 * 60);
        match client.receive_until(deadline, if called_off { &never } else { cancel.as_ref() }) {
            Ok(Some(FromApp::Progress { done, message })) => progress(done.clamp(0.0, 1.0) * 0.9, &message),
            Ok(Some(FromApp::Performed { performed })) => break performed,
            Ok(Some(FromApp::Failed { message })) => return Err(message),
            Ok(Some(_)) => {}
            Ok(None) => {
                return Err(
                    "Shotmaxxing closed before the take finished. What was filmed may be in the Library."
                        .into(),
                );
            }
            Err(_) if !called_off && cancel.load(Ordering::SeqCst) => {
                let _ = sender.send(&ToApp::Cancel);
                called_off = true;
            }
            Err(error) => return Err(format!("lost touch with Shotmaxxing: {error}")),
        }
    };
    let mut finish = |result: TakeResult| {
        let _ = sender.send(&ToApp::Finished { result });
    };
    let root = performed.project.clone();
    if called_off {
        finish(TakeResult {
            beats_done: performed.beats_done,
            stopped: performed.stopped.clone().or_else(|| Some("The agent called the take off.".into())),
            project: Some(root),
            video: None,
            seconds: performed.seconds,
        });
        return Err("cancelled".into());
    }
    let (_, project) = open(&root)?;
    let mut report = summary(&root, &project);
    report["performance"] = json!({
        "beats": performed.beats,
        "beats_done": performed.beats_done,
        "stopped": performed.stopped,
        "captions": performed.captions,
        "handed_over": performed.handed_over,
    });
    report["warnings"] = json!(performed.warnings);
    if performed.stopped.is_none() && script.output.mode == OutputMode::Editor {
        report["note"] = json!("Shotmaxxing is opening it in the editor for the person.");
    }
    if performed.stopped.is_none() && script.output.mode == OutputMode::Video {
        progress(0.9, "Exporting the video");
        let (format, size, fps, aspect) = preset_export(script.output.preset);
        if let Some(aspect) = aspect {
            style(&json!({ "project": root, "aspect": aspect }))?;
        }
        let output = script.output.label.as_deref().map(|label| {
            let name = sanitize_file_name(&format!("{} {label}", project.name));
            unique_path(
                root.parent().unwrap_or(Path::new(".")),
                &name,
                if format == "gif" { "gif" } else { "mp4" },
            )
        });
        let export_args =
            json!({ "project": root, "format": format, "size": size, "fps": fps, "output": output });
        match export(&export_args, &|done, message| progress(0.9 + done * 0.1, message), cancel) {
            Ok(video) => report["video"] = video,
            Err(error) => {
                finish(TakeResult {
                    beats_done: performed.beats_done,
                    stopped: Some(format!("The take is saved, but the video couldn't be exported: {error}")),
                    project: Some(root),
                    video: None,
                    seconds: performed.seconds,
                });
                return Err(error);
            }
        }
    }
    let video = &report["video"];
    finish(TakeResult {
        beats_done: performed.beats_done,
        stopped: performed.stopped.clone(),
        project: Some(root),
        video: video["file"].as_str().map(PathBuf::from),
        seconds: video["seconds"].as_f64().unwrap_or(performed.seconds),
    });
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

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
