//! Captions: the speech model's one-time download, transcribing a project's voices on a background thread with
//! progress for the editor, and subtitle files. Everything runs on this computer; the only request out is the model.
//! Jobs take turns, so two editors never download at once or hold two models in memory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, TryLockError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use shotmaxxing_captions::{CaptionError, MODEL};
use shotmaxxing_core::bundle::sanitize_file_name;
use shotmaxxing_render::RenderError;
use tauri::{AppHandle, Emitter, EventTarget, Manager, Runtime, State, WebviewWindow};

use crate::recording::expand_home;

/// Caption jobs in progress, by the id the editor gave them.
#[derive(Default)]
pub struct Captioning {
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Held by the job that's working; the others wait for it.
    turn: Mutex<()>,
}

impl Captioning {
    /// Whether captions are being made or wait their turn.
    pub fn busy(&self) -> bool {
        self.jobs.lock().is_ok_and(|jobs| !jobs.is_empty())
    }

    /// Stops every job, as the app quits.
    pub fn cancel_all(&self) {
        if let Ok(jobs) = self.jobs.lock() {
            for flag in jobs.values() {
                flag.store(true, Ordering::Relaxed);
            }
        }
    }
}

/// Where the speech model is kept: the app's own data, not the projects folder.
fn models_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    app.path().app_local_data_dir().map(|dir| dir.join("models")).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptionModel {
    /// Why this computer can't make captions, when it can't.
    unsupported: Option<String>,
    /// Downloaded and checked, so captions start at once.
    ready: bool,
    /// The download's size in bytes.
    bytes: u64,
}

/// Whether captions can be made here, and whether the model has to be downloaded first.
#[tauri::command]
pub fn caption_model<R: Runtime>(app: AppHandle<R>) -> Result<CaptionModel, String> {
    let dir = models_dir(&app)?;
    Ok(CaptionModel {
        unsupported: shotmaxxing_captions::supported().err().map(|e| e.to_string()),
        ready: shotmaxxing_captions::model_ready(&dir),
        bytes: MODEL.bytes,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptionRequest {
    id: String,
    /// The project's folder.
    project: String,
    /// Whisper's code for the language spoken ("en"), or none to have it heard.
    #[serde(default)]
    language: Option<String>,
    /// How many units a row of the frame holds (the editor's `rowUnits`), so captions fit in two.
    row_units: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress<'a> {
    id: &'a str,
    /// "waiting" (for another job), "download", "listen" or "transcribe".
    stage: &'static str,
    /// From 0 to 1, within the stage.
    progress: f64,
}

/// A caption made from speech, on its voice's timeline.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MadeCaption {
    start_us: u64,
    end_us: u64,
    text: String,
    /// The voice-over it was heard in, by its file: timed on the finished video.
    #[serde(skip_serializing_if = "Option::is_none")]
    voice_over: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum CaptionOutcome {
    Done {
        captions: Vec<MadeCaption>,
        /// Whisper's code for the language it heard first.
        language: Option<String>,
    },
    Cancelled,
}

/// Makes captions for a project from its voices, downloading the model first if it isn't here.
#[tauri::command]
pub async fn generate_captions<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    captioning: State<'_, Captioning>,
    request: CaptionRequest,
) -> Result<CaptionOutcome, String> {
    let cancel = Arc::new(AtomicBool::new(false));
    captioning
        .jobs
        .lock()
        .map_err(|_| "captions are unavailable".to_string())?
        .insert(request.id.clone(), Arc::clone(&cancel));
    let id = request.id.clone();
    let label = window.label().to_string();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let captioning = app.state::<Captioning>();
        let emit = |stage: &'static str, progress: f64| {
            let target = EventTarget::webview_window(&label);
            let _ = app.emit_to(target, "caption-progress", Progress { id: &request.id, stage, progress });
        };
        // Wait for any other job, checking for a cancel.
        let mut waited = false;
        let _turn = loop {
            match captioning.turn.try_lock() {
                Ok(turn) => break turn,
                Err(TryLockError::Poisoned(poisoned)) => break poisoned.into_inner(),
                Err(TryLockError::WouldBlock) => {
                    if cancel.load(Ordering::Relaxed) {
                        return Ok(CaptionOutcome::Cancelled);
                    }
                    if !waited {
                        waited = true;
                        emit("waiting", 0.0);
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        };
        run(&app, &label, &request, &cancel, &emit)
    })
    .await;
    if let Ok(mut jobs) = captioning.jobs.lock() {
        jobs.remove(&id);
    }
    result.map_err(|e| e.to_string())?
}

fn run<R: Runtime>(
    app: &AppHandle<R>,
    window: &str,
    request: &CaptionRequest,
    cancel: &Arc<AtomicBool>,
    emit: &dyn Fn(&'static str, f64),
) -> Result<CaptionOutcome, String> {
    shotmaxxing_captions::supported().map_err(|e| e.to_string())?;
    let dir = models_dir(app)?;
    let mut last: Option<Instant> = None;
    let fetched = shotmaxxing_captions::fetch_model(&dir, &mut |got, total| {
        // About eight updates a second is smooth enough for a progress bar.
        if last.is_none_or(|at| at.elapsed() >= Duration::from_millis(120)) {
            last = Some(Instant::now());
            emit("download", if total > 0 { (got as f64 / total as f64).min(1.0) } else { 0.0 });
        }
        !cancel.load(Ordering::Relaxed)
    });
    let model = match fetched {
        Ok(model) => model,
        Err(CaptionError::Cancelled) => return Ok(CaptionOutcome::Cancelled),
        Err(error) => {
            tracing::warn!(%error, "the speech model didn't download");
            return Err(error.to_string());
        }
    };

    emit("listen", 0.0);
    let voices =
        match shotmaxxing_render::speech(Path::new(&request.project), &|| cancel.load(Ordering::Relaxed)) {
            Ok(voices) => voices,
            Err(RenderError::Cancelled) => return Ok(CaptionOutcome::Cancelled),
            Err(error) => return Err(error.to_string()),
        };
    if voices.is_empty() {
        return Err("there's no voice to caption: record with a microphone, or add a voice-over".into());
    }

    // Progress across all the voices, by how long each is.
    let total = voices.iter().map(|v| v.samples.len() as f64).sum::<f64>().max(1.0);
    let most = 2 * request.row_units;
    let mut done = 0.0;
    let mut captions = Vec::new();
    let mut language = None;
    for voice in &voices {
        let share = voice.samples.len() as f64 / total;
        let (app, window, id, before) = (app.clone(), window.to_string(), request.id.clone(), done);
        let report: Arc<dyn Fn(f64) + Send + Sync> = Arc::new(move |progress: f64| {
            let target = EventTarget::webview_window(&window);
            let progress = before + share * progress;
            let _ =
                app.emit_to(target, "caption-progress", Progress { id: &id, stage: "transcribe", progress });
        });
        let heard = shotmaxxing_captions::transcribe(
            &model,
            &voice.samples,
            &voice.quiet,
            request.language.as_deref(),
            report,
            Arc::clone(cancel),
        );
        let transcript = match heard {
            Ok(transcript) => transcript,
            Err(CaptionError::Cancelled) => return Ok(CaptionOutcome::Cancelled),
            Err(error) => {
                tracing::warn!(%error, "transcribing failed");
                return Err(error.to_string());
            }
        };
        done += share;
        language = language.or(transcript.language);
        for line in shotmaxxing_captions::lines(&transcript.words, most) {
            // On the voice's own timeline: the microphone can start a moment before the screen.
            let (start, end) = (line.start + voice.offset, line.end + voice.offset);
            if end <= 0.0 || end <= start {
                continue;
            }
            captions.push(MadeCaption {
                start_us: (start.max(0.0) * 1e6).round() as u64,
                end_us: (end * 1e6).round() as u64,
                text: line.text,
                voice_over: voice.voice_over.clone(),
            });
        }
    }
    emit("transcribe", 1.0);
    Ok(CaptionOutcome::Done { captions, language: request.language.clone().or(language) })
}

#[tauri::command]
pub fn cancel_captions(captioning: State<'_, Captioning>, id: String) {
    if let Ok(jobs) = captioning.jobs.lock()
        && let Some(flag) = jobs.get(&id)
    {
        flag.store(true, Ordering::Relaxed);
    }
}

/// Saves a project's captions as a subtitle file (`format` "srt" or "vtt") where the person picks, starting in
/// `folder` (the export folder, or next to the project). Returns where it went, or nothing when they cancelled.
#[tauri::command]
pub async fn save_captions<R: Runtime>(
    app: AppHandle<R>,
    window: WebviewWindow<R>,
    project: String,
    name: String,
    folder: Option<String>,
    format: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let vtt = match format.as_str() {
        "srt" => false,
        "vtt" => true,
        other => return Err(format!("{other} isn't a subtitle format")),
    };
    let root = PathBuf::from(&project);
    let cues = tauri::async_runtime::spawn_blocking(move || shotmaxxing_render::caption_cues(&root))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    if cues.is_empty() {
        return Err("there are no captions to save yet".into());
    }
    let home = app.path().home_dir().ok();
    let start = match folder.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
        Some(folder) => expand_home(folder, home.as_deref()),
        None => Path::new(&project).parent().map(Path::to_path_buf).unwrap_or_default(),
    };
    let extension = if vtt { "vtt" } else { "srt" };
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Save captions")
        .set_parent(&window)
        .set_file_name(format!("{}.{extension}", sanitize_file_name(&name)))
        .add_filter(if vtt { "WebVTT subtitles" } else { "SubRip subtitles" }, &[extension]);
    if start.is_dir() {
        dialog = dialog.set_directory(&start);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    dialog.save_file(move |picked| {
        let _ = tx.send(picked);
    });
    let picked = tauri::async_runtime::spawn_blocking(move || rx.recv().ok().flatten())
        .await
        .map_err(|e| e.to_string())?;
    let Some(mut path) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    if path.extension().is_none() {
        path.set_extension(extension);
    }
    let text =
        if vtt { shotmaxxing_core::captions::vtt(&cues) } else { shotmaxxing_core::captions::srt(&cues) };
    std::fs::write(&path, text).map_err(|e| format!("couldn't save {}: {e}", path.display()))?;
    Ok(Some(path.to_string_lossy().into_owned()))
}
