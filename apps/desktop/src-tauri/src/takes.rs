//! Agent takes as the app sees them. The `shotmaxxing` command films a take; before it starts, it asks here, and
//! the person answers on the take card. The card then follows the take live, and its Stop and Hand back buttons
//! reach the take straight away. Once the person allows an agent's session, its later takes start without asking
//! (the card still shows them).

use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Duration;

use serde::Serialize;
use shotmaxxing_control::{FromApp, Peer, Sender, Server, TakeRequest, TakeResult, ToApp};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::{recording, windows};

/// How long the card waits for an answer before saying no for the person.
const ASK_SECONDS: u64 = 120;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Asking,
    Running,
    Waiting,
    Done,
    Stopped,
}

/// What the take card shows.
#[derive(Clone, Debug, Serialize)]
pub struct Card {
    pub id: u64,
    pub phase: Phase,
    pub client: String,
    pub app: String,
    pub beats: Vec<String>,
    /// The beat in progress, counting from 0; the number of beats while the take is being saved.
    pub beat: usize,
    pub done: f64,
    /// About how long the take runs, in seconds.
    pub seconds: f64,
    /// What the person was asked to do, while the take waits for them.
    pub waiting: Option<String>,
    pub result: Option<TakeResult>,
    #[serde(skip)]
    session: String,
}

impl Card {
    fn live(&self) -> bool {
        matches!(self.phase, Phase::Asking | Phase::Running | Phase::Waiting)
    }
}

struct Answer {
    allowed: bool,
    /// Also allow the agent's later takes, for as long as its session lasts.
    session: bool,
}

#[derive(Default)]
pub struct Takes(Mutex<Inner>);

#[derive(Default)]
struct Inner {
    next: u64,
    card: Option<Card>,
    /// Waiting for the person's answer.
    answer: Option<mpsc::Sender<Answer>>,
    /// The command filming the take, for Stop and Hand back.
    peer: Option<Sender<FromApp>>,
    /// Agent sessions the person allowed for as long as they last.
    sessions: HashSet<String>,
}

/// Starts listening for the `shotmaxxing` command. Without the socket (another copy listening, or no home folder),
/// takes simply run without a card.
pub fn listen<R: Runtime>(app: &AppHandle<R>) {
    let Some(path) = shotmaxxing_control::socket_path() else { return };
    let server = match Server::bind(&path) {
        Ok(server) => server,
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "agents' takes won't show a take card");
            return;
        }
    };
    let app = app.clone();
    let started = std::thread::Builder::new().name("control".into()).spawn(move || {
        server.run(move |peer| serve(&app, peer));
    });
    if let Err(error) = started {
        tracing::warn!(%error, "couldn't start listening for agents' takes");
    }
}

/// One connection from the command: an ask, then the take's progress until it finishes.
fn serve<R: Runtime>(app: &AppHandle<R>, mut peer: Peer) {
    let takes = app.state::<Takes>();
    let mut id = None;
    while let Ok(Some(message)) = peer.receive() {
        match message {
            ToApp::Ask { take } => id = takes.ask(app, &mut peer, take),
            ToApp::Progress { beat, done } => takes.update(app, id, |card| {
                card.phase = Phase::Running;
                card.beat = beat.min(card.beats.len());
                card.done = done.clamp(0.0, 1.0);
                card.waiting = None;
            }),
            ToApp::Waiting { message } => takes.update(app, id, |card| {
                card.phase = Phase::Waiting;
                card.waiting = Some(message);
            }),
            ToApp::Finished { result } => takes.update(app, id, |card| {
                card.phase = if result.stopped.is_some() { Phase::Stopped } else { Phase::Done };
                card.beat = result.beats_done.min(card.beats.len());
                card.done = 1.0;
                card.waiting = None;
                card.result = Some(result);
            }),
        }
    }
    // The command went away. A take it hadn't finished ended with it.
    takes.update(app, id, |card| {
        if card.live() {
            card.phase = Phase::Stopped;
            card.waiting = None;
            card.result = Some(TakeResult {
                beats_done: card.beat,
                stopped: Some("The agent stopped before the take finished.".into()),
                ..TakeResult::default()
            });
        }
    });
    if let Ok(mut inner) = takes.0.lock()
        && inner.card.as_ref().is_some_and(|c| Some(c.id) == id)
    {
        inner.peer = None;
        inner.answer = None;
    }
}

impl Takes {
    /// Shows the card and waits for the person's answer, then tells the command. Returns the card's id when the
    /// take may go ahead.
    fn ask<R: Runtime>(&self, app: &AppHandle<R>, peer: &mut Peer, take: TakeRequest) -> Option<u64> {
        let refuse = |peer: &mut Peer, reason: &str| {
            let _ = peer.send(&FromApp::Answer { allowed: false, reason: Some(reason.into()) });
            None
        };
        if app.state::<recording::Recordings>().is_recording() {
            return refuse(peer, "The person is recording in Shotmaxxing. Try again when they're done.");
        }
        let (id, answer) = {
            let Ok(mut inner) = self.0.lock() else {
                return refuse(peer, "Shotmaxxing couldn't show the take.");
            };
            if inner.card.as_ref().is_some_and(Card::live) {
                drop(inner);
                return refuse(peer, "Another take is being filmed. Try again when it's done.");
            }
            inner.next += 1;
            let id = inner.next;
            let allowed = inner.sessions.contains(&take.session);
            inner.card = Some(Card {
                id,
                phase: if allowed { Phase::Running } else { Phase::Asking },
                client: take.client,
                app: take.app,
                beats: take.beats,
                beat: 0,
                done: 0.0,
                seconds: take.seconds,
                waiting: None,
                result: None,
                session: take.session,
            });
            inner.peer = peer.sender().ok();
            let answer = (!allowed).then(|| {
                let (tx, rx) = mpsc::channel();
                inner.answer = Some(tx);
                rx
            });
            (id, answer)
        };
        self.show(app);

        let allowed = match answer {
            None => true,
            Some(answer) => match answer.recv_timeout(Duration::from_secs(ASK_SECONDS)) {
                Ok(Answer { allowed, session }) => {
                    if allowed
                        && session
                        && let Ok(mut inner) = self.0.lock()
                        && let Some(card) = inner.card.as_ref().filter(|c| c.id == id)
                    {
                        let session = card.session.clone();
                        inner.sessions.insert(session);
                    }
                    allowed
                }
                Err(_) => {
                    self.forget(app, id);
                    return refuse(
                        peer,
                        "Nobody answered in Shotmaxxing within two minutes, so the take didn't start.",
                    );
                }
            },
        };
        if !allowed {
            self.forget(app, id);
            return refuse(peer, "The person said no to this take in Shotmaxxing.");
        }
        self.update(app, Some(id), |card| card.phase = Phase::Running);
        let _ = peer.send(&FromApp::Answer { allowed: true, reason: None });
        Some(id)
    }

    /// Changes the card, if it's still the one with `id`, and shows the change.
    fn update<R: Runtime>(&self, app: &AppHandle<R>, id: Option<u64>, change: impl FnOnce(&mut Card)) {
        let card = {
            let Ok(mut inner) = self.0.lock() else { return };
            let Some(card) = inner.card.as_mut().filter(|c| Some(c.id) == id) else { return };
            change(card);
            card.clone()
        };
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

    /// Drops the card with `id` and closes its window.
    fn forget<R: Runtime>(&self, app: &AppHandle<R>, id: u64) {
        if let Ok(mut inner) = self.0.lock()
            && inner.card.as_ref().is_some_and(|c| c.id == id)
        {
            inner.card = None;
            inner.answer = None;
            inner.peer = None;
        }
        if let Some(window) = app.get_webview_window("take") {
            let _ = window.close();
        }
    }

    fn send(&self, id: u64, message: FromApp) -> Result<(), String> {
        let mut inner = self.0.lock().map_err(|_| "Shotmaxxing couldn't reach the take.".to_string())?;
        if !inner.card.as_ref().is_some_and(|c| c.id == id && c.live()) {
            return Err("That take has ended.".into());
        }
        let peer = inner.peer.as_mut().ok_or("That take has ended.")?;
        peer.send(&message).map_err(|_| "The agent has gone away.".to_string())
    }
}

/// What the take card shows now, if there's a take.
#[tauri::command]
pub fn take_card(takes: State<'_, Takes>) -> Option<Card> {
    takes.0.lock().ok().and_then(|inner| inner.card.clone())
}

/// The person's answer: allow the take (and, with `session`, the agent's later takes) or not.
#[tauri::command]
pub fn answer_take(takes: State<'_, Takes>, id: u64, allow: bool, session: bool) -> Result<(), String> {
    let inner = takes.0.lock().map_err(|_| "Shotmaxxing couldn't reach the take.".to_string())?;
    if !inner.card.as_ref().is_some_and(|c| c.id == id && c.phase == Phase::Asking) {
        return Err("That take isn't waiting for an answer any more.".into());
    }
    let answer = inner.answer.as_ref().ok_or("That take isn't waiting for an answer any more.")?;
    answer
        .send(Answer { allowed: allow, session })
        .map_err(|_| "That take isn't waiting any more.".to_string())
}

/// Stops the take now. What was filmed so far is kept as a project.
#[tauri::command]
pub fn stop_take(takes: State<'_, Takes>, id: u64) -> Result<(), String> {
    takes.send(id, FromApp::Stop)
}

/// Gives control back to the take after a hand-over, like pressing ⌥⌘Return.
#[tauri::command]
pub fn hand_back_take(takes: State<'_, Takes>, id: u64) -> Result<(), String> {
    takes.send(id, FromApp::HandBack)
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
