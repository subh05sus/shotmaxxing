//! Agents in the app. The `shotmaxxing` command connects for each thing an agent asks that takes the system's
//! permissions (listing windows, looking at an app before a take, recording, filming a take), and the app does it with
//! its own, so the agent's terminal needs none.
//!
//! The person decides first, on the take card: once per agent session before an agent may see or record the screen,
//! and before every take. The card names who asked as the system reports it (the connecting process, the agent that
//! started it, the app that runs in and who signed that app), never as the request claims. A take then plays out on
//! the card, where Stop and Hand back reach it straight away, under the plan the agent showed the person, and a red
//! edge goes round the screen while the agent drives.
//!
//! With Allow everything on ([`Trust`]), the card doesn't ask: the shotmaxxing command's requests go through, and
//! the card still follows each take with its buttons. Everything else stays: Esc three times or moving the mouse
//! stops a take, password fields are never typed into, and the red edge shows.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde::Serialize;
use shotmaxxing_capture::{Target, Thumbnail};
use shotmaxxing_control::{
    Caller, FromApp, PROTOCOL, Peer, Performed, Picture, Sender, Server, TakePlan, TakeRequest, TakeResult,
    ToApp,
};
use shotmaxxing_core::take::OutputMode;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::{edge, recording, tray, windows};

/// How long the card waits for an answer before saying no for the person.
const ASK_SECONDS: u64 = 120;
/// How long a request waits for another agent's to finish before giving up. The command allows for it too
/// (`BUSY_SECONDS` in crates/cli/src/mcp.rs).
const BUSY_SECONDS: u64 = 120;

/// The widest picture of a window sent to an agent.
const PICTURE_WIDTH: u32 = 1280;

/// How long a finished take's card shows it's done before the take opens in the editor.
const SEEN: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// An agent asks to see and record the screen for the rest of its session.
    Access,
    /// An agent's take.
    Take,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Asking,
    Running,
    /// Paused from the card, until resumed.
    Paused,
    /// The person has control (a `user_step`, or they took over), until they hand back.
    Waiting,
    Done,
    Stopped,
}

/// A hold the person asked for on the card. It starts when the beat in progress ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Hold {
    Pause,
    TakeOver,
}

/// What the take card shows.
#[derive(Clone, Debug, Serialize)]
pub struct Card {
    pub id: u64,
    pub kind: Kind,
    pub phase: Phase,
    /// The agent's name for itself, e.g. "Claude Code".
    pub client: String,
    /// Who connected, as the system reports it.
    pub caller: Option<Caller>,
    /// The app a take films.
    pub app: String,
    /// What an agent asking for access wants to do first, e.g. "look at TextEdit".
    pub purpose: String,
    pub beats: Vec<String>,
    /// The beat in progress, counting from 0; the number of beats while the take is being saved.
    pub beat: usize,
    pub done: f64,
    /// About how long the take runs, in seconds.
    pub seconds: f64,
    /// What the person was asked to do, while the take waits for them.
    pub waiting: Option<String>,
    /// The hold the person asked for, from the moment they ask until it's lifted.
    pub hold: Option<Hold>,
    pub result: Option<TakeResult>,
    /// What the take is for, as the agent showed the person before filming.
    pub plan: Option<Plan>,
    /// Allow everything let it through without asking.
    pub trusted: bool,
}

impl Card {
    fn live(&self) -> bool {
        matches!(self.phase, Phase::Asking | Phase::Running | Phase::Paused | Phase::Waiting)
    }

    /// Whether the agent has the pointer and keyboard right now: a take playing, not on hold or being saved.
    fn driving(&self) -> bool {
        self.kind == Kind::Take && self.phase == Phase::Running && self.beat < self.beats.len()
    }
}

/// A take's plan on the card, tidied: a title and plain steps, each one short line.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Plan {
    pub title: String,
    pub steps: Vec<String>,
}

/// The most steps a card shows, and how long a step and the title may be.
const PLAN_STEPS: usize = 12;
const STEP_CHARS: usize = 140;
const TITLE_CHARS: usize = 60;

impl Plan {
    /// The plan an agent sent, made safe to show; `None` when it has no steps.
    fn tidy(plan: &TakePlan) -> Option<Self> {
        let steps: Vec<String> = plan
            .steps
            .iter()
            .map(|step| one_line(step, STEP_CHARS))
            .filter(|step| !step.is_empty())
            .take(PLAN_STEPS)
            .collect();
        (!steps.is_empty()).then(|| Self { title: one_line(&plan.title, TITLE_CHARS), steps })
    }
}

/// Allow everything, from Settings: agents' requests go through without the card asking, until it's turned off or
/// Shotmaxxing quits. It's kept in memory only, so no program can switch it on by changing a file.
#[derive(Default)]
pub struct Trust(AtomicBool);

impl Trust {
    pub fn on(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// Whether `who` goes through without asking: Allow everything is on, and the system says the shotmaxxing
    /// command connected, so a program speaking the protocol by itself still asks.
    fn lets(&self, who: &Who) -> bool {
        self.on() && who.caller.as_ref().is_some_and(Caller::is_command)
    }
}

/// The take being filmed, for the card's buttons.
#[derive(Clone, Default)]
struct Flags {
    cancel: Arc<AtomicBool>,
    hand_back: Arc<AtomicBool>,
    hold: Arc<AtomicU8>,
}

/// An agent session the person let see and record the screen: the session the command named, and the process the
/// system says connected, so a session can't be borrowed by another program.
type Allowed = (String, Option<u32>);

#[derive(Default)]
pub struct Takes(Mutex<Inner>);

/// What a connection waits for: the person's answer on the card, or the command.
enum Event {
    Answer(bool),
    Message(ToApp),
    /// The command went away.
    Gone,
}

/// One connection after its request: replies go out through `reply`, and the command's later messages arrive as
/// events beside the person's answers, so a request the agent calls off while the card asks goes away at once.
struct Conn {
    who: Who,
    reply: Sender<FromApp>,
    events: mpsc::Sender<Event>,
    rx: mpsc::Receiver<Event>,
}

impl Conn {
    fn send(&mut self, message: &FromApp) -> bool {
        self.reply.send(message).is_ok()
    }
}

#[derive(Default)]
struct Inner {
    next: u64,
    card: Option<Card>,
    /// Where the person's answer goes, while the card asks.
    answer: Option<mpsc::Sender<Event>>,
    /// The take being filmed: which card, and its flags.
    filming: Option<(u64, Flags)>,
    allowed: HashSet<Allowed>,
}

/// Who is asking, from the hello and from the system.
struct Who {
    client: String,
    session: String,
    caller: Option<Caller>,
}

impl Who {
    fn key(&self) -> Allowed {
        (self.session.clone(), self.caller.as_ref().map(|c| c.pid))
    }

    fn card(&self, kind: Kind) -> Card {
        Card {
            id: 0,
            kind,
            phase: Phase::Asking,
            client: self.client.clone(),
            caller: self.caller.clone(),
            app: String::new(),
            purpose: String::new(),
            beats: Vec::new(),
            beat: 0,
            done: 0.0,
            seconds: 0.0,
            waiting: None,
            hold: None,
            result: None,
            plan: None,
            trusted: false,
        }
    }
}

/// An agent's name as it gave it, made safe to show: one line, at most 40 characters.
fn tidy_name(name: &str) -> String {
    let name = one_line(name, 40);
    if name.is_empty() { "An agent".into() } else { name }
}

/// Text an agent sent, made safe to show: one line, its spaces collapsed, at most `most` characters.
fn one_line(text: &str, most: usize) -> String {
    let flat: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let flat = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > most {
        format!("{}…", flat.chars().take(most - 1).collect::<String>())
    } else {
        flat
    }
}

/// Starts listening for the `shotmaxxing` command. Without the socket (another copy listening, or no home folder),
/// agents can't reach the app and say so.
pub fn listen<R: Runtime>(app: &AppHandle<R>) {
    let Some(path) = shotmaxxing_control::socket_path() else { return };
    let server = match Server::bind(&path) {
        Ok(server) => server,
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "agents can't reach this copy of Shotmaxxing");
            return;
        }
    };
    let app = app.clone();
    let started = std::thread::Builder::new().name("control".into()).spawn(move || {
        server.run(move |peer| serve(&app, peer));
    });
    if let Err(error) = started {
        tracing::warn!(%error, "couldn't start listening for agents");
    }
}

/// One connection from the command: a hello, then one request.
fn serve<R: Runtime>(app: &AppHandle<R>, mut peer: Peer) {
    let caller = peer.caller();
    let who = match peer.receive() {
        Ok(Some(ToApp::Hello { protocol, client, session })) => {
            let welcome = FromApp::Welcome { protocol: PROTOCOL, version: env!("CARGO_PKG_VERSION").into() };
            if peer.send(&welcome).is_err() || protocol != PROTOCOL {
                return;
            }
            Who { client: tidy_name(&client), session, caller }
        }
        _ => return,
    };
    let Ok(Some(request)) = peer.receive() else { return };
    let Ok(reply) = peer.sender() else { return };
    let (events, rx) = mpsc::channel();
    let reader = events.clone();
    let reading = std::thread::Builder::new().name("control reader".into()).spawn(move || {
        while let Ok(Some(message)) = peer.receive() {
            if reader.send(Event::Message(message)).is_err() {
                return;
            }
        }
        let _ = reader.send(Event::Gone);
    });
    if reading.is_err() {
        return;
    }
    let mut conn = Conn { who, reply, events, rx };
    let takes = app.state::<Takes>();
    let answer = match request {
        ToApp::Sources => takes.allow(app, &conn, "see which windows are open").and_then(|()| {
            let sources = shotmaxxing_capture::sources().map_err(|e| e.to_string())?;
            let sources = serde_json::to_value(sources).map_err(|e| e.to_string())?;
            Ok(FromApp::Sources { sources })
        }),
        ToApp::Prepare { app: target, window, picture } => {
            let purpose = match target.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
                Some(target) => format!("look at {target}"),
                None => "look at the app in front".to_string(),
            };
            takes
                .allow(app, &conn, &purpose)
                .and_then(|()| prepare(target.as_deref(), window.as_deref(), picture))
        }
        ToApp::Take { take } => {
            film(app, &mut conn, *take);
            conn.reply.close();
            return;
        }
        ToApp::Record { record } => takes.allow(app, &conn, "record the screen").and_then(|()| {
            let status = recording::start_for_agent(app, conn.who.key(), record)?;
            Ok(FromApp::Recording { status: Some(status) })
        }),
        ToApp::RecordingStatus => {
            Ok(FromApp::Recording { status: recording::agent_status(app, &conn.who.key()) })
        }
        ToApp::StopRecording { discard } => {
            recording::stop_for_agent(app, &conn.who.key(), discard).map(|recorded| match recorded {
                Some(recorded) => FromApp::Recorded { recorded },
                None => FromApp::Recording { status: None },
            })
        }
        ToApp::Open { project } => open_project(app, &project),
        _ => Err("That isn't something the Shotmaxxing app does.".into()),
    };
    conn.send(&answer.unwrap_or_else(|message| FromApp::Failed { message }));
    conn.reply.close();
}

/// Opens a saved project in the editor. Nothing is filmed or driven, so it doesn't ask.
fn open_project<R: Runtime>(app: &AppHandle<R>, project: &Path) -> Result<FromApp, String> {
    if let Err(error) = shotmaxxing_core::bundle::ProjectBundle::open(project) {
        return Err(format!("Shotmaxxing can't open {} as a project: {error}", project.display()));
    }
    let (handle, project) = (app.clone(), project.to_path_buf());
    app.run_on_main_thread(move || {
        if let Err(error) = windows::open_project(&handle, &project) {
            tracing::warn!(%error, "couldn't open the project in the editor");
        }
    })
    .map_err(|_| "Shotmaxxing couldn't open the editor.".to_string())?;
    Ok(FromApp::Opened)
}

/// Reads an app's controls, and takes a picture of its window.
fn prepare(target: Option<&str>, window: Option<&str>, picture: bool) -> Result<FromApp, String> {
    let prepared = shotmaxxing_agent::prepare(target, window).map_err(|e| e.to_string())?;
    let picture = match (&prepared.window, picture) {
        // A window Stage Manager parked has no picture worth sending (it would be small and tilted), and one on
        // another Space can't be pictured from this one.
        (Some(window), true) => shotmaxxing_capture::thumbnails(&[Target::Window(window.id)], PICTURE_WIDTH)
            .ok()
            .and_then(|pictures| pictures.into_iter().next().flatten())
            .and_then(|picture| match picture {
                Thumbnail::Picture(jpeg) => Some(Picture::from_jpeg(&jpeg)),
                Thumbnail::Parked { .. } | Thumbnail::Away { .. } => None,
            }),
        _ => None,
    };
    let prepared = serde_json::to_value(prepared).map_err(|e| e.to_string())?;
    Ok(FromApp::Prepared { prepared, picture })
}

/// Films a take once the person allows it, then follows it on the card until the command is done with it.
fn film<R: Runtime>(app: &AppHandle<R>, conn: &mut Conn, take: TakeRequest) {
    let takes = app.state::<Takes>();
    let refuse = |conn: &mut Conn, reason: String| {
        conn.send(&FromApp::Answer { allowed: false, reason: Some(reason) });
    };
    const RECORDING: &str = "Shotmaxxing is recording. Stop that recording before filming a take.";
    if app.state::<recording::Recordings>().is_recording() {
        return refuse(conn, RECORDING.into());
    }
    let mut card = conn.who.card(Kind::Take);
    card.app = take.script.app.clone().unwrap_or_default();
    card.beats = take.beats.clone();
    card.seconds = take.seconds;
    card.plan = take.plan.as_ref().and_then(Plan::tidy);
    let id = if app.state::<Trust>().lets(&conn.who) {
        card.trusted = true;
        match takes.put_up(app, conn, card, Phase::Running) {
            Ok(id) => id,
            Err(reason) => return refuse(conn, reason),
        }
    } else {
        match takes.ask(app, conn, card) {
            Ok((id, true)) => id,
            Ok((id, false)) => {
                takes.forget(app, id);
                return refuse(conn, "The person said no to this take in Shotmaxxing.".into());
            }
            Err(reason) => return refuse(conn, reason),
        }
    };
    // The person may have started recording while the card asked.
    if app.state::<recording::Recordings>().is_recording() {
        takes.forget(app, id);
        return refuse(conn, RECORDING.into());
    }
    // Only a command that's still there to hear it starts a take.
    let Ok(sender) = conn.reply.try_clone() else {
        takes.forget(app, id);
        return;
    };
    if !conn.send(&FromApp::Answer { allowed: true, reason: None }) {
        takes.forget(app, id);
        return;
    }
    let flags = Flags::default();
    takes.start(app, id, flags.clone());
    // A script asks for the editor unless it says otherwise.
    let opens_editor = take.script.output.mode == OutputMode::Editor;

    // The take plays on a thread of its own, and this one keeps listening for the agent calling it off.
    let performer = {
        let (app, flags, client) = (app.clone(), flags.clone(), conn.who.client.clone());
        std::thread::Builder::new()
            .name("agent take".into())
            .spawn(move || perform(&app, id, take, client, flags, sender))
    };
    let performer = match performer {
        Ok(performer) => performer,
        Err(error) => {
            tracing::warn!(%error, "couldn't start the take");
            let message = "Shotmaxxing couldn't start the take.".to_string();
            takes.end(app, id, None, Some(message.clone()));
            conn.send(&FromApp::Failed { message });
            return;
        }
    };
    let mut finished = false;
    loop {
        match conn.rx.recv() {
            Ok(Event::Message(ToApp::Cancel)) => flags.cancel.store(true, Ordering::SeqCst),
            Ok(Event::Message(ToApp::Finished { result })) => {
                let open = result.project.clone().filter(|_| opens_editor && result.stopped.is_none());
                takes.finish(app, id, result);
                finished = true;
                if let Some(project) = open {
                    takes.open_when_seen(app, id, project);
                }
            }
            Ok(Event::Message(_) | Event::Answer(_)) => {}
            Ok(Event::Gone) | Err(_) => break,
        }
    }
    // The agent went away. A take still playing stops there, and what was filmed is kept.
    let went_early = !performer.is_finished();
    flags.cancel.store(true, Ordering::SeqCst);
    let performed = performer.join().ok().flatten();
    if !finished {
        let reason = went_early.then(|| "The agent stopped before the take finished.".to_string());
        takes.end(app, id, performed, reason);
    }
}

/// Plays the take. Returns how it went, once it's saved.
fn perform<R: Runtime>(
    app: &AppHandle<R>,
    id: u64,
    take: TakeRequest,
    client: String,
    flags: Flags,
    mut sender: Sender<FromApp>,
) -> Option<Performed> {
    let takes = app.state::<Takes>();
    let beats = take.beats.len();
    let options = shotmaxxing_agent::TakeOptions {
        folder: take.folder,
        planned: take.planned,
        microphone: recording::device_choice(take.microphone.as_deref()),
        system_audio: take.system_audio,
        client: Some(client),
        cancel: flags.cancel.clone(),
        hand_back: flags.hand_back.clone(),
        hold: flags.hold.clone(),
    };
    let result = shotmaxxing_agent::run_take(&take.script, options, &mut |p| {
        // A Hand back pressed before the take asked for one doesn't count.
        if p.waiting.is_some() {
            flags.hand_back.store(false, Ordering::SeqCst);
        }
        takes.update(app, id, |card| {
            match (&p.waiting, p.paused, p.beat) {
                (Some(message), _, _) => {
                    card.phase = Phase::Waiting;
                    card.waiting = Some(message.clone());
                    return;
                }
                (None, true, _) => {
                    card.phase = Phase::Paused;
                    card.waiting = None;
                    return;
                }
                (None, false, Some(beat)) => card.beat = beat.min(beats),
                // Every beat played: the take is being saved.
                (None, false, None) if p.done >= 1.0 => card.beat = beats,
                (None, false, None) => {}
            }
            card.phase = Phase::Running;
            card.waiting = None;
            card.done = p.done.clamp(0.0, 1.0);
            // Moving again with nothing on hold: a hold the person asked for is over.
            if flags.hold.load(Ordering::SeqCst) == shotmaxxing_agent::hold::NONE {
                card.hold = None;
            }
        });
        let _ = sender.send(&FromApp::Progress { done: p.done, message: p.message.clone() });
    });
    match result {
        Ok(take) => {
            app.state::<recording::Notices>().remember(&take.finished);
            let mut warnings = take.finished.warnings.clone();
            warnings.extend(take.performed.warnings.iter().cloned());
            let performed = Performed {
                project: take.finished.root.clone(),
                seconds: take.finished.project.duration_us() as f64 / 1_000_000.0,
                beats: take.performed.beats,
                beats_done: take.performed.beats_done,
                stopped: take.performed.stopped.clone(),
                captions: take.performed.captions.len(),
                handed_over: take.performed.handoffs.len(),
                warnings,
            };
            takes.update(app, id, |card| {
                card.phase = Phase::Running;
                card.beat = beats;
                card.done = 1.0;
                card.waiting = None;
            });
            let _ = sender.send(&FromApp::Performed { performed: performed.clone() });
            Some(performed)
        }
        Err(error) => {
            let message = error.to_string();
            takes.end(app, id, None, Some(message.clone()));
            let _ = sender.send(&FromApp::Failed { message });
            None
        }
    }
}

impl Takes {
    /// Whether an agent's take is being filmed right now.
    pub fn is_filming(&self) -> bool {
        self.0.lock().is_ok_and(|inner| inner.filming.is_some())
    }

    /// Lets the session see and record the screen, asking the person the first time. `Err` says why not.
    fn allow<R: Runtime>(&self, app: &AppHandle<R>, conn: &Conn, purpose: &str) -> Result<(), String> {
        let key = conn.who.key();
        // Not remembered for the session: turning Allow everything off asks again from the next request.
        if app.state::<Trust>().lets(&conn.who) || self.0.lock().map_err(|_| busy())?.allowed.contains(&key) {
            return Ok(());
        }
        let mut card = conn.who.card(Kind::Access);
        card.purpose = purpose.to_string();
        let (id, allowed) = self.ask(app, conn, card)?;
        self.forget(app, id);
        if !allowed {
            return Err(format!(
                "The person said no in Shotmaxxing, so {} can't see or record the screen in this session.",
                conn.who.client
            ));
        }
        self.0.lock().map_err(|_| busy())?.allowed.insert(key);
        Ok(())
    }

    /// Puts `card` up and waits for the person's answer: the card's id, and whether they allowed it. The card goes
    /// away by itself when the agent calls the request off or goes away, or after two minutes.
    fn ask<R: Runtime>(&self, app: &AppHandle<R>, conn: &Conn, card: Card) -> Result<(u64, bool), String> {
        let id = self.put_up(app, conn, card, Phase::Asking)?;
        let deadline = Instant::now() + Duration::from_secs(ASK_SECONDS);
        loop {
            match conn.rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(Event::Answer(allowed)) => return Ok((id, allowed)),
                Ok(Event::Message(ToApp::Cancel) | Event::Gone) => {
                    self.forget(app, id);
                    return Err("The agent called it off.".into());
                }
                Ok(Event::Message(_)) => {}
                Err(_) => {
                    self.forget(app, id);
                    return Err("Nobody answered in Shotmaxxing within two minutes.".into());
                }
            }
        }
    }

    /// Puts `card` up in `phase`, and returns its id: asking, with the person's answer going to `conn`, or already
    /// running when Allow everything let it through.
    fn put_up<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        conn: &Conn,
        mut card: Card,
        phase: Phase,
    ) -> Result<u64, String> {
        // Another agent's request on the card goes first: this one waits its turn rather than failing, so two agents
        // (or an agent filming while another prepares) take turns. It gives up after a while, or when called off.
        let give_up = Instant::now() + Duration::from_secs(BUSY_SECONDS);
        let id = loop {
            {
                let mut inner = self.0.lock().map_err(|_| busy())?;
                if !inner.card.as_ref().is_some_and(Card::live) {
                    inner.next += 1;
                    card.id = inner.next;
                    card.phase = phase;
                    let id = card.id;
                    inner.card = Some(card);
                    inner.answer = (phase == Phase::Asking).then(|| conn.events.clone());
                    break id;
                }
            }
            if Instant::now() >= give_up {
                return Err(format!(
                    "Shotmaxxing is still busy with another agent's request after {BUSY_SECONDS} seconds. Try again \
                     when it's done."
                ));
            }
            if let Ok(Event::Message(ToApp::Cancel) | Event::Gone) =
                conn.rx.recv_timeout(Duration::from_millis(250))
            {
                return Err("The agent called it off.".into());
            }
        };
        self.show(app);
        Ok(id)
    }

    /// The take on card `id` starts.
    fn start<R: Runtime>(&self, app: &AppHandle<R>, id: u64, flags: Flags) {
        if let Ok(mut inner) = self.0.lock() {
            inner.answer = None;
            inner.filming = Some((id, flags));
        }
        self.update(app, id, |card| card.phase = Phase::Running);
    }

    /// The command finished its work on the take: the card shows how it ended.
    fn finish<R: Runtime>(&self, app: &AppHandle<R>, id: u64, result: TakeResult) {
        self.stop_filming(id);
        self.update(app, id, |card| {
            card.phase = if result.stopped.is_some() { Phase::Stopped } else { Phase::Done };
            card.beat = result.beats_done.min(card.beats.len());
            card.done = 1.0;
            card.waiting = None;
            card.result = Some(result);
        });
    }

    /// Opens a finished take in the editor, as its script asked, once the card has shown it's done, then closes the
    /// card. If the editor can't open, the card stays with its own Open in editor button.
    fn open_when_seen<R: Runtime>(&self, app: &AppHandle<R>, id: u64, project: PathBuf) {
        let app = app.clone();
        let opening = std::thread::Builder::new().name("open take".into()).spawn(move || {
            std::thread::sleep(SEEN);
            let handle = app.clone();
            let opened = app.run_on_main_thread(move || match windows::open_project(&handle, &project) {
                Ok(()) => handle.state::<Takes>().forget(&handle, id),
                Err(error) => tracing::warn!(%error, "couldn't open the take in the editor"),
            });
            if let Err(error) = opened {
                tracing::warn!(%error, "couldn't open the take in the editor");
            }
        });
        if let Err(error) = opening {
            tracing::warn!(%error, "couldn't open the take in the editor");
        }
    }

    /// The take ended without the command finishing it: from what the app knows, and why it stopped, if it did.
    fn end<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        id: u64,
        performed: Option<Performed>,
        reason: Option<String>,
    ) {
        self.stop_filming(id);
        self.update(app, id, |card| {
            if !card.live() {
                return;
            }
            let result = match performed {
                Some(performed) => TakeResult {
                    beats_done: performed.beats_done,
                    stopped: reason.or(performed.stopped),
                    project: Some(performed.project),
                    video: None,
                    seconds: performed.seconds,
                },
                None => TakeResult { beats_done: card.beat, stopped: reason, ..TakeResult::default() },
            };
            card.phase = if result.stopped.is_some() { Phase::Stopped } else { Phase::Done };
            card.beat = result.beats_done.min(card.beats.len());
            card.done = 1.0;
            card.waiting = None;
            card.result = Some(result);
        });
    }

    /// Puts the take on hold, or lifts the hold, from the card's buttons.
    fn hold<R: Runtime>(&self, app: &AppHandle<R>, id: u64, hold: Option<Hold>) -> Result<(), String> {
        let value = match hold {
            None => shotmaxxing_agent::hold::NONE,
            Some(Hold::Pause) => shotmaxxing_agent::hold::PAUSE,
            Some(Hold::TakeOver) => shotmaxxing_agent::hold::TAKE_OVER,
        };
        self.flags(id)?.hold.store(value, Ordering::SeqCst);
        self.update(app, id, |card| card.hold = hold);
        Ok(())
    }

    fn stop_filming(&self, id: u64) {
        if let Ok(mut inner) = self.0.lock()
            && inner.filming.as_ref().is_some_and(|(filming, _)| *filming == id)
        {
            inner.filming = None;
        }
    }

    /// Changes the card, if it's still the one with `id`, and shows the change.
    fn update<R: Runtime>(&self, app: &AppHandle<R>, id: u64, change: impl FnOnce(&mut Card)) {
        let card = {
            let Ok(mut inner) = self.0.lock() else { return };
            let Some(card) = inner.card.as_mut().filter(|c| c.id == id) else { return };
            change(card);
            card.clone()
        };
        edge::set(app, card.driving());
        let _ = app.emit_to("take", "take-card", &card);
    }

    /// Opens the card's window, or brings it back, and shows the latest state.
    fn show<R: Runtime>(&self, app: &AppHandle<R>) {
        let app = app.clone();
        let card = self.0.lock().ok().and_then(|inner| inner.card.clone());
        let opened = app.clone().run_on_main_thread(move || {
            if let Err(error) = windows::open_take_card(&app) {
                tracing::warn!(%error, "couldn't open the take card");
            }
            if let Some(card) = card {
                let _ = app.emit_to("take", "take-card", &card);
            }
        });
        if let Err(error) = opened {
            tracing::warn!(%error, "couldn't open the take card");
        }
    }

    /// Whether the card asks something or follows a take.
    pub fn is_live(&self) -> bool {
        self.0.lock().is_ok_and(|inner| inner.card.as_ref().is_some_and(Card::live))
    }

    /// Puts the card's window back if it closed while the card still asks or a take plays.
    pub fn keep_shown<R: Runtime>(&self, app: &AppHandle<R>) {
        if self.is_live() {
            self.show(app);
        }
    }

    /// Drops the card with `id` and closes its window.
    fn forget<R: Runtime>(&self, app: &AppHandle<R>, id: u64) {
        let forgotten = match self.0.lock() {
            Ok(mut inner) if inner.card.as_ref().is_some_and(|c| c.id == id) => {
                inner.card = None;
                inner.answer = None;
                true
            }
            _ => false,
        };
        if forgotten {
            edge::set(app, false);
            if let Some(window) = app.get_webview_window("take") {
                let _ = window.close();
            }
        }
    }

    /// The flags of the take on card `id`, while it plays.
    fn flags(&self, id: u64) -> Result<Flags, String> {
        let inner = self.0.lock().map_err(|_| busy())?;
        match &inner.filming {
            Some((filming, flags)) if *filming == id => Ok(flags.clone()),
            _ => Err("That take has ended.".into()),
        }
    }
}

fn busy() -> String {
    "Shotmaxxing couldn't reach the take card.".into()
}

/// Whether Allow everything is on.
#[tauri::command]
pub fn agent_trust(trust: State<'_, Trust>) -> bool {
    trust.on()
}

/// Turns Allow everything on or off, from Settings, the Agents page or the take card. It lasts until it's turned
/// off or Shotmaxxing quits; the menu-bar icon wears a red dot while it's on, and every window hears of the change.
#[tauri::command]
pub fn set_agent_trust<R: Runtime>(app: AppHandle<R>, trust: State<'_, Trust>, on: bool) {
    if trust.0.swap(on, Ordering::SeqCst) != on {
        tracing::info!(on, "Allow everything");
    }
    tray::set_trusted(&app, on);
    let _ = app.emit("agent-trust", on);
}

/// What the take card shows now, if anything.
#[tauri::command]
pub fn take_card(takes: State<'_, Takes>) -> Option<Card> {
    takes.0.lock().ok().and_then(|inner| inner.card.clone())
}

/// The person's answer to what the card asks: a take, or access to the screen for an agent's session.
#[tauri::command]
pub fn answer_take(takes: State<'_, Takes>, id: u64, allow: bool) -> Result<(), String> {
    let inner = takes.0.lock().map_err(|_| busy())?;
    if !inner.card.as_ref().is_some_and(|c| c.id == id && c.phase == Phase::Asking) {
        return Err("That request isn't waiting for an answer any more.".into());
    }
    let answer = inner.answer.as_ref().ok_or("That request isn't waiting for an answer any more.")?;
    answer.send(Event::Answer(allow)).map_err(|_| "That request isn't waiting any more.".to_string())
}

/// Stops the take now. What was filmed so far is kept as a project.
#[tauri::command]
pub fn stop_take(takes: State<'_, Takes>, id: u64) -> Result<(), String> {
    takes.flags(id)?.cancel.store(true, Ordering::SeqCst);
    Ok(())
}

/// Pauses the take once the step in progress ends. The pause is cut from the video.
#[tauri::command]
pub fn pause_take<R: Runtime>(app: AppHandle<R>, takes: State<'_, Takes>, id: u64) -> Result<(), String> {
    takes.hold(&app, id, Some(Hold::Pause))
}

/// Carries on after a pause.
#[tauri::command]
pub fn resume_take<R: Runtime>(app: AppHandle<R>, takes: State<'_, Takes>, id: u64) -> Result<(), String> {
    takes.hold(&app, id, None)
}

/// Hands control to the person once the step in progress ends, until they hand back. The stretch is cut from the
/// video.
#[tauri::command]
pub fn take_over_take<R: Runtime>(app: AppHandle<R>, takes: State<'_, Takes>, id: u64) -> Result<(), String> {
    takes.hold(&app, id, Some(Hold::TakeOver))
}

/// Gives control back to the take after a hand-over, like pressing ⌥⌘Return.
#[tauri::command]
pub fn hand_back_take(takes: State<'_, Takes>, id: u64) -> Result<(), String> {
    takes.flags(id)?.hand_back.store(true, Ordering::SeqCst);
    Ok(())
}

/// Clears a finished take's card.
#[tauri::command]
pub fn dismiss_take(takes: State<'_, Takes>, id: u64) {
    if let Ok(mut inner) = takes.0.lock()
        && inner.card.as_ref().is_some_and(|c| c.id == id && !c.live())
    {
        inner.card = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caller(program: &str) -> Caller {
        Caller {
            pid: 42,
            program: program.into(),
            path: format!("/Applications/Shotmaxxing.app/Contents/MacOS/{program}"),
            agent: Some("claude".into()),
            app: Some("Terminal".into()),
            signer: Some("Apple".into()),
            checked: true,
        }
    }

    #[test]
    fn allow_everything_lets_only_the_command_through() {
        let trust = Trust::default();
        let who = |caller: Option<Caller>| Who { client: "Claude Code".into(), session: "s".into(), caller };
        assert!(!trust.lets(&who(Some(caller("shotmaxxing")))), "off by default");
        trust.0.store(true, Ordering::SeqCst);
        assert!(trust.lets(&who(Some(caller("shotmaxxing")))));
        assert!(trust.lets(&who(Some(caller("shotmaxxing.exe")))));
        // Another program speaking the protocol, or one the system can't name, still asks.
        assert!(!trust.lets(&who(Some(caller("python3")))));
        assert!(!trust.lets(&who(None)));
    }

    #[test]
    fn plans_are_tidied_for_the_card() {
        let plan = TakePlan {
            title: "  Sign-up\nflow ".into(),
            steps: vec!["Click  Get started".into(), "\t".into(), "x".repeat(300)],
        };
        let tidy = Plan::tidy(&plan).unwrap();
        assert_eq!(tidy.title, "Sign-up flow");
        assert_eq!(tidy.steps.len(), 2, "the blank step goes");
        assert_eq!(tidy.steps[0], "Click Get started");
        assert_eq!(tidy.steps[1].chars().count(), STEP_CHARS);
        let many = TakePlan { title: String::new(), steps: (0..30).map(|n| format!("Step {n}")).collect() };
        assert_eq!(Plan::tidy(&many).unwrap().steps.len(), PLAN_STEPS);
        assert_eq!(Plan::tidy(&TakePlan { title: "Empty".into(), steps: vec![" ".into()] }), None);
    }

    #[test]
    fn the_agent_drives_only_while_a_take_plays() {
        let mut card = Who { client: "A".into(), session: "s".into(), caller: None }.card(Kind::Take);
        card.beats = vec!["Click".into(), "Type".into()];
        assert!(!card.driving(), "asking");
        card.phase = Phase::Running;
        assert!(card.driving());
        card.beat = 2;
        assert!(!card.driving(), "being saved");
        card.beat = 1;
        for phase in [Phase::Paused, Phase::Waiting, Phase::Done, Phase::Stopped] {
            card.phase = phase;
            assert!(!card.driving(), "{phase:?}");
        }
        card.phase = Phase::Running;
        card.kind = Kind::Access;
        assert!(!card.driving(), "access doesn't move anything");
    }

    #[test]
    fn agents_names_are_one_short_line() {
        assert_eq!(tidy_name("  Claude\nCode  "), "Claude Code");
        assert_eq!(tidy_name(""), "An agent");
        assert_eq!(tidy_name("\u{7}\t"), "An agent");
        let long = "x".repeat(80);
        assert_eq!(tidy_name(&long).chars().count(), 40);
        assert!(tidy_name(&long).ends_with('…'));
    }
}
