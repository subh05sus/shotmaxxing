//! `shotmaxxing`: record from the terminal, re-plan auto zoom, check take scripts, and (soon) serve AI agents.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use shotmaxxing_capture::{CURSOR_FILE, EVENTS_FILE, ProjectRecording, Target};
use shotmaxxing_core::bundle::ProjectBundle;
use shotmaxxing_core::project::{Project, ZoomDepth};
use shotmaxxing_core::take::TakeScript;
use shotmaxxing_core::telemetry::{decode_cursor, read_events};
use shotmaxxing_core::timeparse::{format_timecode, parse_duration_us};
use shotmaxxing_core::zoom::{ZoomInput, plan_zooms};

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
    /// Record a screen or a window into a new project.
    Record(RecordArgs),
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
    /// Check or describe take scripts.
    #[command(subcommand)]
    Take(TakeCommand),
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
}

#[derive(Subcommand)]
enum TakeCommand {
    /// Check a take script (YAML or JSON) and list any problems.
    Validate { file: PathBuf },
    /// Print the JSON Schema for take scripts.
    Schema,
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
        Command::Record(args) => record(args, cli.json),
        Command::Info { project } => info(&project, cli.json),
        Command::Zoom { project, depth } => zoom(&project, depth.into(), cli.json),
        Command::Take(TakeCommand::Validate { file }) => validate(&file, cli.json),
        Command::Take(TakeCommand::Schema) => {
            println!("{}", serde_json::to_string_pretty(&shotmaxxing_core::take::json_schema())?);
            Ok(())
        }
    }
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
    let recording = ProjectRecording::start(&folder, &args.name, target, args.fps.clamp(1, 120), None)?;
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
            }))?
        );
    } else {
        print_summary(&finished.root, &finished.project);
        if finished.dropped > 0 {
            println!("  {} frames were dropped; the machine was busy.", finished.dropped);
        }
    }
    Ok(())
}

fn print_summary(root: &Path, project: &Project) {
    println!("{}", project.name);
    println!("  {}", root.display());
    println!(
        "  {} · {}×{} pt at {}×",
        format_timecode(project.duration_us(), true),
        project.source.size.width,
        project.source.size.height,
        project.source.scale
    );
    if project.zooms.is_empty() {
        println!("  No zooms: nothing was clicked or typed.");
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
        print_summary(bundle.root(), &project);
    }
    Ok(())
}

fn zoom(path: &Path, depth: ZoomDepth, json: bool) -> Result<()> {
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
    if json {
        println!("{}", serde_json::to_string_pretty(&project.zooms)?);
    } else {
        print_summary(bundle.root(), &project);
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
