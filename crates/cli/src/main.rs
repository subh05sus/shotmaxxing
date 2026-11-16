//! `shotmaxxing`: record from the terminal, export finished videos, re-plan auto zoom, check take scripts, and
//! serve AI agents over MCP.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use shotmaxxing_capture::{CURSOR_FILE, DeviceChoice, EVENTS_FILE, ProjectOptions, ProjectRecording, Target};
use shotmaxxing_core::bundle::ProjectBundle;
use shotmaxxing_core::project::{Project, TrackKind, ZoomDepth};
use shotmaxxing_core::take::{OutputMode, TakeScript};
use shotmaxxing_core::telemetry::{decode_cursor, read_events};
use shotmaxxing_core::timeparse::{format_timecode, parse_duration_us};
use shotmaxxing_core::zoom::{ZoomInput, plan_zooms};
use shotmaxxing_render::{
    ExportOptions, Format, RenderProject, default_bitrate, output_size, png_size, unique_path,
};

mod mcp;

#[derive(Parser)]
#[command(
    name = "shotmaxxing",
    version,
    about = "Record demos that edit themselves.",
    propagate_version = true
)]
struct Cli {
    /// Print machine-readable JSON instead of text.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the screens and windows you can record.
    Sources,
    /// List the microphones and cameras you can record.
    Devices,
    /// Record a screen or a window into a new project.
    Record(RecordArgs),
    /// Export a project to a video, with every edit applied.
    Export(ExportArgs),
    /// Show what's in a project.
    Info {
        /// A .smx project folder.
        project: PathBuf,
    },
    /// Plan auto zoom again from a project's recorded clicks and typing.
    Zoom {
        project: PathBuf,
        #[arg(long, value_enum, default_value_t = Depth::Standard)]
        depth: Depth,
    },
    /// Open a project in the Shotmaxxing editor, opening the app first if it isn't running.
    Open {
        /// A .smx project folder.
        project: PathBuf,
    },
    /// Check or describe take scripts.
    #[command(subcommand)]
    Take(TakeCommand),
    /// Check content packs, the wallpapers and looks the app's Settings can add.
    #[command(subcommand)]
    Pack(PackCommand),
    /// Serve AI agents over MCP, on stdin and stdout. Agents start this themselves once connected.
    Mcp,
}

#[derive(Args)]
struct RecordArgs {
    /// A display id from `shotmaxxing sources` (the main display if nothing is chosen).
    #[arg(long, conflicts_with_all = ["window", "app"])]
    display: Option<u32>,
    /// A window id from `shotmaxxing sources`.
    #[arg(long, conflicts_with = "app")]
    window: Option<u32>,
    /// Record the largest window of this app, e.g. "Safari".
    #[arg(long)]
    app: Option<String>,
    /// Stop after this long: 30s, 1:30 or 2m. Without it, press Ctrl-C to stop.
    #[arg(long, short = 'd')]
    duration: Option<String>,
    /// The project name.
    #[arg(long, default_value = "Recording")]
    name: String,
    /// The folder to put the project in.
    #[arg(long)]
    out: Option<PathBuf>,
    /// The most frames per second. The real rate follows what changes on screen.
    #[arg(long, default_value_t = 60)]
    fps: u32,
    /// Leave the desktop icons out of a screen recording.
    #[arg(long)]
    hide_desktop_icons: bool,
    /// Record what the computer plays.
    #[arg(long)]
    system_audio: bool,
    /// Record a microphone: an id from `shotmaxxing devices`, or the system's own choice when no id is given.
    #[arg(long, value_name = "ID", num_args = 0..=1, default_missing_value = "default")]
    mic: Option<String>,
    /// Film yourself with a camera: an id from `shotmaxxing devices`, or the system's own choice when no id is
    /// given.
    #[arg(long, value_name = "ID", num_args = 0..=1, default_missing_value = "default")]
    camera: Option<String>,
}

fn device_choice(choice: Option<String>) -> DeviceChoice {
    match choice.as_deref() {
        None => DeviceChoice::Off,
        Some("default") => DeviceChoice::Default,
        Some(id) => DeviceChoice::Id(id.to_string()),
    }
}

#[derive(Args)]
struct ExportArgs {
    /// A .smx project folder.
    project: PathBuf,
    /// The file to write. Without it, the video goes next to the project, named after it.
    #[arg(long, short = 'o')]
    out: Option<PathBuf>,
    /// MP4 plays everywhere; HEVC is smaller; GIF loops in chats and docs; PNG is one styled frame. Defaults to MP4,
    /// or PNG for a screenshot.
    #[arg(long, value_enum)]
    format: Option<OutFormat>,
    /// For a PNG: the moment to show, in seconds into the recording (like the editor's playhead).
    #[arg(long, default_value_t = 0.0)]
    at: f64,
    /// The shorter side in pixels, e.g. 720, 1080, 1440 or 2160. The other side follows the canvas shape. Defaults to
    /// 1080, or for a screenshot its own pixels, one for one.
    #[arg(long)]
    size: Option<u32>,
    /// Frames per second.
    #[arg(long, default_value_t = 60)]
    fps: u32,
    /// Bitrate in Mbit/s. The default suits the size.
    #[arg(long)]
    mbps: Option<f64>,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutFormat {
    Mp4,
    Hevc,
    Gif,
    Png,
}

#[derive(Subcommand)]
enum TakeCommand {
    /// Check a take script (YAML or JSON) and list any problems.
    Validate { file: PathBuf },
    /// Print the JSON Schema for take scripts.
    Schema,
    /// Check a take script against the app on screen now: what would stop the take, with something to try instead.
    Check {
        file: PathBuf,
        /// Part of the window's title, when the app has several.
        #[arg(long)]
        window: Option<String>,
    },
    /// Open an app and list the controls a take script can name by ref.
    Prepare {
        /// The app, e.g. "Safari", or its bundle id. Without it, the app in front.
        #[arg(long)]
        app: Option<String>,
        /// Part of the window's title, when the app has several.
        #[arg(long)]
        window: Option<String>,
    },
    /// Film a take script: Shotmaxxing performs it while recording. Press Esc three times to stop.
    Run {
        file: PathBuf,
        /// Show the timeline without moving anything.
        #[arg(long)]
        dry_run: bool,
        /// The folder to put the project in.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Record narration: a microphone id from `shotmaxxing devices`, or the system's choice.
        #[arg(long, value_name = "ID", num_args = 0..=1, default_missing_value = "default")]
        mic: Option<String>,
        /// Record what the computer plays.
        #[arg(long)]
        system_audio: bool,
    },
}

#[derive(Subcommand)]
enum PackCommand {
    /// Check a content pack file the way the app does before adding it, and say what's in it.
    Check { file: PathBuf },
    /// List the content packs added in the app, with the ids of their wallpapers.
    List,
}

#[derive(Clone, Copy, ValueEnum)]
enum Depth {
    Subtle,
    Standard,
    Bold,
}

impl From<Depth> for ZoomDepth {
    fn from(depth: Depth) -> Self {
        match depth {
            Depth::Subtle => ZoomDepth::Subtle,
            Depth::Standard => ZoomDepth::Standard,
            Depth::Bold => ZoomDepth::Bold,
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();
    let cli = Cli::parse();
    if let Err(error) = run(cli) {
        eprintln!("shotmaxxing: {error:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Sources => sources(cli.json),
        Command::Devices => devices(cli.json),
        Command::Record(args) => record(args, cli.json),
        Command::Export(args) => export(args, cli.json),
        Command::Info { project } => info(&project, cli.json),
        Command::Open { project } => open_in_editor(&project, cli.json),
        Command::Zoom { project, depth } => zoom(&project, depth.into(), cli.json),
        Command::Take(TakeCommand::Validate { file }) => validate(&file, cli.json),
        Command::Take(TakeCommand::Check { file, window }) => check_take(&file, window.as_deref(), cli.json),
        Command::Take(TakeCommand::Prepare { app, window }) => {
            prepare(app.as_deref(), window.as_deref(), cli.json)
        }
        Command::Take(TakeCommand::Run { file, dry_run, out, mic, system_audio }) => {
            run_take(&file, dry_run, out, mic, system_audio, cli.json)
        }
        Command::Mcp => mcp::serve(),
        Command::Pack(PackCommand::Check { file }) => check_pack(&file, cli.json),
        Command::Pack(PackCommand::List) => list_packs(cli.json),
        Command::Take(TakeCommand::Schema) => {
            println!("{}", serde_json::to_string_pretty(&shotmaxxing_core::take::json_schema())?);
            Ok(())
        }
    }
}

fn export(args: ExportArgs, json: bool) -> Result<()> {
    let project = RenderProject::load(&args.project).context("couldn't open the project")?;
    let format = match args.format.unwrap_or(if project.still { OutFormat::Png } else { OutFormat::Mp4 }) {
        OutFormat::Mp4 => Format::H264,
        OutFormat::Hevc => Format::Hevc,
        OutFormat::Gif => Format::Gif,
        OutFormat::Png => Format::Png,
    };
    // A screenshot comes out pixel for pixel (0 × 0) unless a size is asked for.
    let (width, height) = match args.size {
        None if project.still => (0, 0),
        size => output_size(project.canvas_aspect(), size.unwrap_or(1080).clamp(240, 4320)),
    };
    let fps = args.fps.clamp(1, 120);
    let output = match args.out {
        Some(out) => out,
        None => {
            let dir = args.project.parent().unwrap_or(Path::new("."));
            unique_path(dir, &shotmaxxing_core::bundle::sanitize_file_name(&project.name), format.extension())
        }
    };
    let bitrate = args.mbps.map_or_else(|| default_bitrate(width, height, fps, format), |mbps| mbps * 1e6);
    let at = (format == Format::Png).then_some(args.at);
    let options = ExportOptions { width, height, fps, format, bitrate, output: output.clone(), at };

    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    ctrlc::set_handler(move || flag.store(true, Ordering::SeqCst)).context("couldn't watch for Ctrl-C")?;
    let started = Instant::now();
    let mut shown = -1;
    let result = shotmaxxing_render::export(&args.project, &options, &mut |done| {
        let percent = (done * 100.0) as i32;
        if !json && percent != shown {
            eprint!("\rExporting {width}×{height} at {fps} fps… {percent}%");
            shown = percent;
        }
        !stop.load(Ordering::SeqCst)
    });
    if !json {
        eprintln!();
    }
    result.context("the export didn't finish")?;
    let seconds = started.elapsed().as_secs_f64();
    let (width, height) = if width == 0 { png_size(&output).unwrap_or_default() } else { (width, height) };
    if json {
        let report = serde_json::json!({
            "output": output, "width": width, "height": height, "fps": fps,
            "duration_s": project.output_duration(), "took_s": seconds,
        });
        println!("{report}");
    } else if project.still {
        println!("Saved {} ({width} × {height})", output.display());
    } else {
        println!(
            "Saved {} ({:.1} s of video in {seconds:.1} s)",
            output.display(),
            project.output_duration()
        );
    }
    Ok(())
}

fn default_folder() -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    let base = home.unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        base.join("Movies").join("Shotmaxxing")
    } else {
        base.join("Videos").join("Shotmaxxing")
    }
}

fn devices(json: bool) -> Result<()> {
    let microphones = shotmaxxing_capture::microphones();
    let cameras = shotmaxxing_capture::cameras();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({ "microphones": microphones, "cameras": cameras })
            )?
        );
        return Ok(());
    }
    for (heading, list) in [("Microphones", &microphones), ("Cameras", &cameras)] {
        println!("{heading}");
        if list.is_empty() {
            println!("  None found");
        }
        for d in list {
            println!("  {}{}  {}", d.name, if d.is_default { " (default)" } else { "" }, d.id);
        }
        println!();
    }
    println!("Record with them: shotmaxxing record --mic [<id>] --camera [<id>] --system-audio");
    Ok(())
}

fn sources(json: bool) -> Result<()> {
    let sources = shotmaxxing_capture::sources()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&sources)?);
        return Ok(());
    }
    println!("Screens");
    for d in &sources.displays {
        println!("  {:>6}  {}  {}×{}", d.id, d.name, d.width, d.height);
    }
    println!("\nWindows");
    for w in &sources.windows {
        let title = if w.title.is_empty() { String::new() } else { format!(" — {}", w.title) };
        println!("  {:>6}  {}{}  {}×{}", w.id, w.app, title, w.width, w.height);
    }
    println!("\nRecord one with: shotmaxxing record --display <id>  or  --window <id>  or  --app <name>");
    Ok(())
}

fn record(args: RecordArgs, json: bool) -> Result<()> {
    let target = if let Some(id) = args.window {
        Target::Window(id)
    } else if let Some(app) = &args.app {
        let sources = shotmaxxing_capture::sources()?;
        let needle = app.to_lowercase();
        let window = sources
            .windows
            .iter()
            .filter(|w| w.app.to_lowercase().contains(&needle))
            .max_by(|a, b| (a.width * a.height).total_cmp(&(b.width * b.height)))
            .with_context(|| {
                format!("no open window belongs to “{app}”. Run `shotmaxxing sources` to see them.")
            })?;
        Target::Window(window.id)
    } else {
        Target::Display(args.display)
    };
    let limit = args
        .duration
        .as_deref()
        .map(|d| parse_duration_us(d).map(Duration::from_micros))
        .transpose()
        .map_err(|e| anyhow::anyhow!("couldn't read the duration: {e}"))?;

    let stop = Arc::new(AtomicBool::new(false));
    let handler_stop = stop.clone();
    ctrlc::set_handler(move || handler_stop.store(true, Ordering::Relaxed))
        .context("couldn't listen for Ctrl-C")?;

    let folder = args.out.unwrap_or_else(default_folder);
    let mut options = ProjectOptions::new(target);
    options.max_fps = args.fps.clamp(1, 120);
    options.hide_desktop_icons = args.hide_desktop_icons;
    options.system_audio = args.system_audio;
    options.microphone = device_choice(args.mic);
    options.camera = device_choice(args.camera);
    let recording = ProjectRecording::start(&folder, &args.name, options)?;
    if !json {
        eprintln!("Recording to {}", recording.root().display());
        eprintln!(
            "{}",
            if limit.is_some() { "Stops by itself, or press Ctrl-C." } else { "Press Ctrl-C to stop." }
        );
    }
    let mut shown = 0;
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let elapsed = recording.elapsed();
        if stop.load(Ordering::Relaxed) || limit.is_some_and(|l| elapsed >= l.as_secs_f64()) {
            break;
        }
        let whole = elapsed as u64;
        if !json && whole != shown {
            shown = whole;
            eprint!("\r  ● {}", format_timecode(whole * 1_000_000, false));
        }
    }
    if !json {
        eprintln!("\rStopping…        ");
    }
    let finished = recording.stop()?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "project": finished.root,
                "duration_us": finished.project.duration_us(),
                "frames": finished.frames,
                "dropped_frames": finished.dropped,
                "zooms": finished.project.zooms,
                "tracks": finished.project.tracks,
                "warnings": finished.warnings,
            }))?
        );
    } else {
        print_summary(&finished.root, &finished.project, finished.project.zoom.auto);
        if finished.dropped > 0 {
            println!("  {} frames were dropped; the machine was busy.", finished.dropped);
        }
        for warning in &finished.warnings {
            println!("  {warning}");
        }
    }
    Ok(())
}

/// What a project holds. `planned` says its zooms were planned just now, so an empty plan can say why.
fn print_summary(root: &Path, project: &Project, planned: bool) {
    println!("{}", project.name);
    println!("  {}", root.display());
    println!(
        "  {} · {}×{} pt at {}×",
        format_timecode(project.duration_us(), true),
        project.source.size.width,
        project.source.size.height,
        project.source.scale
    );
    let extras: Vec<&str> = project
        .tracks
        .iter()
        .filter_map(|t| match t.kind {
            TrackKind::SystemAudio => Some("the computer's sound"),
            TrackKind::Microphone => Some("the microphone"),
            TrackKind::Camera => Some("the camera"),
            _ => None,
        })
        .collect();
    if !extras.is_empty() {
        println!("  With {}", extras.join(", "));
    }
    if project.zooms.is_empty() {
        // A saved project's zooms may have been taken out by hand, so only a plan made just now says why.
        if planned {
            println!("  No zooms: nothing small enough to zoom in on was clicked or typed into.");
        } else {
            println!("  No zooms.");
        }
    } else {
        println!("  Auto zoom added {}:", project.zooms.len());
        for z in &project.zooms {
            println!(
                "    {}–{}  {:.1}×  {}",
                format_timecode(z.start_us, true),
                format_timecode(z.end_us, true),
                z.scale,
                z.reason.as_deref().unwrap_or("")
            );
        }
    }
}

fn info(path: &Path, json: bool) -> Result<()> {
    let (bundle, project) =
        ProjectBundle::open(path).with_context(|| format!("couldn't open {}", path.display()))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&project)?);
    } else {
        print_summary(bundle.root(), &project, false);
    }
    Ok(())
}

fn zoom(path: &Path, depth: ZoomDepth, json: bool) -> Result<()> {
    let (root, project) = replan(path, depth)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&project.zooms)?);
    } else {
        print_summary(&root, &project, true);
    }
    Ok(())
}

/// Plans a project's automatic zooms again from its recorded clicks and typing, keeping a version first.
fn replan(path: &Path, depth: ZoomDepth) -> Result<(PathBuf, Project)> {
    let (bundle, mut project) =
        ProjectBundle::open(path).with_context(|| format!("couldn't open {}", path.display()))?;
    let cursor = std::fs::read(bundle.root().join(CURSOR_FILE))
        .ok()
        .map(|bytes| decode_cursor(&bytes))
        .transpose()?
        .unwrap_or_default();
    let events = match std::fs::File::open(bundle.root().join(EVENTS_FILE)) {
        Ok(file) => read_events(std::io::BufReader::new(file))?,
        Err(_) => Vec::new(),
    };
    if cursor.is_empty() && events.is_empty() {
        bail!("this project has no recorded clicks or typing to plan from");
    }
    bundle.snapshot(&project, "Before planning auto zoom again")?;
    project.zoom.depth = depth;
    let input = ZoomInput {
        duration_us: project.duration_us(),
        capture: project.source.size,
        events: &events,
        cursor: &cursor,
    };
    project.zooms = plan_zooms(&input, &project.zoom.tuning, depth, project.zoom.motion.transition_ms());
    bundle.save(&mut project)?;
    Ok((bundle.root().to_path_buf(), project))
}

fn prepare(app: Option<&str>, window: Option<&str>, json: bool) -> Result<()> {
    let prepared = shotmaxxing_agent::prepare(app, window)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&prepared)?);
        return Ok(());
    }
    if let Some(window) = &prepared.window {
        println!(
            "{} · {} ({:.0}×{:.0} at {:.0}, {:.0})",
            prepared.app, window.title, window.width, window.height, window.x, window.y
        );
    }
    for c in &prepared.controls {
        let state = if c.enabled { "" } else { "  (disabled)" };
        println!("  {:<40} {}{state}", c.id, c.label.as_deref().unwrap_or(""));
    }
    if prepared.truncated {
        println!("  … and more");
    }
    Ok(())
}

fn run_take(
    file: &Path,
    dry_run: bool,
    out: Option<PathBuf>,
    mic: Option<String>,
    system_audio: bool,
    json: bool,
) -> Result<()> {
    let text = std::fs::read_to_string(file).with_context(|| format!("couldn't read {}", file.display()))?;
    let script = TakeScript::parse(&text).map_err(|e| anyhow::anyhow!("{}: {e}", file.display()))?;
    if dry_run {
        let plan = shotmaxxing_agent::plan(&script, &[]);
        if json {
            println!("{}", serde_json::to_string_pretty(&plan)?);
        } else {
            for beat in &plan.beats {
                println!("{:>7.2} s  {}", beat.at_seconds, beat.what);
            }
            println!("{:>7.2} s  end", plan.seconds);
            for note in &plan.notes {
                println!("note: {note}");
            }
        }
        return Ok(());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    ctrlc::set_handler(move || flag.store(true, Ordering::SeqCst)).context("couldn't watch for Ctrl-C")?;
    if !json {
        eprintln!("Performing {}. Press Esc three times, or move the mouse, to stop.", file.display());
    }
    let options = shotmaxxing_agent::TakeOptions {
        folder: out.unwrap_or_else(default_folder),
        planned: Vec::new(),
        microphone: device_choice(mic),
        system_audio,
        client: Some("Shotmaxxing CLI".into()),
        cancel,
        hand_back: Default::default(),
        hold: Default::default(),
    };
    let take = shotmaxxing_agent::run_take(&script, options, &mut |p| {
        if !json {
            eprintln!("{}", p.message);
        }
    })?;
    let root = take.finished.root.clone();
    if !json {
        if let Some(reason) = &take.performed.stopped {
            println!("Stopped: {reason}");
        }
        print_summary(&root, &take.finished.project, take.finished.project.zoom.auto);
    }
    // A script asks for the editor unless it says otherwise, and the editor is in the app.
    let opened = (take.performed.stopped.is_none() && script.output.mode == OutputMode::Editor)
        .then(|| mcp::open_in_editor(&root));
    if json {
        let report = serde_json::json!({
            "project": root,
            "performance": take.performed,
            "warnings": take.finished.warnings,
            "opened_in_editor": matches!(opened, Some(Ok(()))),
        });
        println!("{report}");
    }
    match &opened {
        Some(Ok(())) if !json => println!("Opened it in the Shotmaxxing editor."),
        Some(Err(error)) => eprintln!("Couldn't open it in the editor: {error}"),
        _ => {}
    }
    if take.performed.stopped.is_none() && script.output.mode == OutputMode::Video {
        let gif = script.output.preset == Some(shotmaxxing_core::project::ExportPreset::Gif);
        let args = ExportArgs {
            project: root,
            out: None,
            format: Some(if gif { OutFormat::Gif } else { OutFormat::Mp4 }),
            size: Some(if gif { 480 } else { 1080 }),
            fps: if gif { 15 } else { 60 },
            mbps: None,
            at: 0.0,
        };
        export(args, json)?;
    }
    Ok(())
}

fn open_in_editor(project: &Path, json: bool) -> Result<()> {
    mcp::open_in_editor(project).map_err(|e| anyhow::anyhow!("couldn't open {}: {e}", project.display()))?;
    if json {
        println!("{}", serde_json::json!({ "opened": std::path::absolute(project)? }));
    } else {
        println!("Opened {} in the Shotmaxxing editor.", project.display());
    }
    Ok(())
}

fn check_pack(file: &Path, json: bool) -> Result<()> {
    let pack =
        shotmaxxing_render::packs::read_file(file).map_err(|e| anyhow::anyhow!("{}: {e}", file.display()))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&pack)?);
    } else {
        let count = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        println!(
            "{}: looks good. “{}” has {} and {}.",
            file.display(),
            pack.name,
            count(pack.wallpapers.len(), "wallpaper", "wallpapers"),
            count(pack.looks.len(), "look", "looks"),
        );
    }
    Ok(())
}

fn list_packs(json: bool) -> Result<()> {
    let packs = shotmaxxing_render::packs::installed();
    if json {
        println!("{}", serde_json::to_string_pretty(&packs)?);
    } else if packs.is_empty() {
        println!("No content packs are added. Add one in Shotmaxxing's Settings.");
    } else {
        for pack in &packs {
            let ids: Vec<String> = pack.wallpapers.iter().map(|w| pack.wallpaper_id(&w.id)).collect();
            println!(
                "{} ({}): {}",
                pack.name,
                pack.id,
                if ids.is_empty() { "looks only".into() } else { ids.join(", ") }
            );
        }
    }
    Ok(())
}

/// The plan check from the command line: the script against the app on screen now, with this terminal's
/// permissions. Refs are looked up on screen, since there's no earlier `take prepare` to compare with.
fn check_take(file: &Path, window: Option<&str>, json: bool) -> Result<()> {
    let text = std::fs::read_to_string(file).with_context(|| format!("couldn't read {}", file.display()))?;
    let script = TakeScript::parse(&text).map_err(|e| anyhow::anyhow!("{}: {e}", file.display()))?;
    if !script.is_valid() {
        anyhow::bail!("{} has problems: run `shotmaxxing take validate` on it first", file.display());
    }
    let prepared = shotmaxxing_agent::prepare(script.app.as_deref(), window)?;
    let check = shotmaxxing_agent::check(&script, &[], &prepared.controls, prepared.window.as_ref());
    if json {
        println!("{}", serde_json::to_string_pretty(&check)?);
    } else {
        let line = |what: &str, finding: &shotmaxxing_agent::Finding| {
            let beat = if finding.beat == 0 { "take".to_string() } else { format!("beat {}", finding.beat) };
            println!("{}: {beat}: {what}: {}", file.display(), finding.message);
            if let Some(instead) = &finding.instead {
                println!("  try: {instead}");
            }
        };
        check.problems.iter().for_each(|f| line("problem", f));
        check.notes.iter().for_each(|f| line("note", f));
        if check.ready {
            println!(
                "{}: ready: nothing on screen in {} would stop this take.",
                file.display(),
                prepared.app
            );
        }
    }
    if !check.ready {
        std::process::exit(1);
    }
    Ok(())
}

fn validate(file: &Path, json: bool) -> Result<()> {
    let text = std::fs::read_to_string(file).with_context(|| format!("couldn't read {}", file.display()))?;
    let script = TakeScript::parse(&text).map_err(|e| anyhow::anyhow!("{}: {e}", file.display()))?;
    let issues = script.validate();
    if json {
        println!("{}", serde_json::to_string_pretty(&issues)?);
    } else if issues.is_empty() {
        println!("{}: looks good ({} beats).", file.display(), script.beats.len());
    } else {
        for issue in &issues {
            let beat = issue.beat.map(|b| format!("beat {}", b + 1)).unwrap_or_else(|| "script".into());
            println!("{}: {beat}: {:?}: {}", file.display(), issue.severity, issue.message);
        }
    }
    if !script.is_valid() {
        std::process::exit(1);
    }
    Ok(())
}
