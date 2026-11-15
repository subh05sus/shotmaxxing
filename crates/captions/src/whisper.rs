//! Transcription with whisper.cpp: the model loads for each request and is freed after, so nothing stays in memory
//! between captions.

use std::ffi::c_void;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::{CaptionError, SAMPLE_RATE, Transcript, Word};

/// A word being read: its bytes, when it starts and ends, and whether a space comes before it (`None` at a segment's
/// start, decided once its text is known).
type Partial = (Vec<u8>, f64, f64, Option<bool>);

/// Letters of scripts written without spaces between words: Chinese characters, hiragana and katakana.
fn unspaced_letter(c: char) -> bool {
    matches!(u32::from(c), 0x3040..=0x30FF | 0x31F0..=0x31FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x3FFFD)
}

/// Those letters, and the punctuation that goes with them (、。「」 and full-width forms).
fn unspaced(c: char) -> bool {
    unspaced_letter(c) || matches!(u32::from(c), 0x3000..=0x303F | 0xFF00..=0xFF65)
}

/// Whether `next` starts a word of its own after `so_far` although no space comes between: in Chinese and Japanese
/// each character can, so captions can break between them. Only between whole characters (a token can hold part of
/// one), and never before punctuation, which stays with what it ends.
fn breaks_before(so_far: &[u8], next: &[u8]) -> bool {
    let (Ok(so_far), Ok(next)) = (std::str::from_utf8(so_far), std::str::from_utf8(next)) else {
        return false;
    };
    so_far.chars().last().is_some_and(unspaced) && next.chars().next().is_some_and(unspaced_letter)
}

/// Whisper's own idea that a stretch held no speech, above which its words are dropped (its default threshold).
const NO_SPEECH: f32 = 0.6;

fn failed(error: whisper_rs::WhisperError) -> CaptionError {
    CaptionError::Whisper(error.to_string())
}

/// Whether the caller cancelled, as whisper.cpp asks between steps (from its worker threads too). whisper-rs 0.16's
/// own `set_abort_callback_safe` calls its closure as the wrong type, so it read stray memory and stopped every
/// transcription at the first step ("failed to encode"); this reads the flag itself.
///
/// # Safety
/// `flag` must point to an `AtomicBool` that lives until whisper.cpp returns.
unsafe extern "C" fn cancelled(flag: *mut c_void) -> bool {
    // SAFETY: as the caller promises.
    unsafe { (*flag.cast::<AtomicBool>()).load(Ordering::Relaxed) }
}

/// Half the cores, from two to eight: enough to be quick, with room left for everything else and a cooler computer.
/// On macOS the GPU does most of the work anyway.
fn threads() -> i32 {
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    (cores / 2).clamp(2, 8) as i32
}

/// Not speech: "[BLANK_AUDIO]", "(music)", "*laughs*", "♪".
fn sound_effect(text: &str) -> bool {
    let pairs = [('[', ']'), ('(', ')'), ('*', '*'), ('♪', '♪')];
    text.chars().all(|c| c == '♪') || pairs.iter().any(|&(a, b)| text.starts_with(a) && text.ends_with(b))
}

pub fn transcribe(
    model: &Path,
    audio: &[f32],
    language: Option<&str>,
    progress: Arc<dyn Fn(f64) + Send + Sync>,
    cancel: Arc<AtomicBool>,
) -> Result<Transcript, CaptionError> {
    // whisper.cpp's own logging goes through `tracing`, not the terminal.
    whisper_rs::install_logging_hooks();
    // Under half a second, there's nothing to caption.
    if audio.len() < SAMPLE_RATE as usize / 2 {
        return Ok(Transcript::default());
    }
    let context_params = WhisperContextParameters {
        use_gpu: cfg!(target_os = "macos"),
        flash_attn: cfg!(target_os = "macos"),
        ..WhisperContextParameters::default()
    };
    let context = WhisperContext::new_with_params(model, context_params).map_err(failed)?;
    let mut state = context.create_state().map_err(failed)?;

    // whisper.cpp's command-line defaults: a beam of five, fewer made-up words than greedy decoding.
    let mut params = FullParams::new(SamplingStrategy::BeamSearch { beam_size: 5, patience: -1.0 });
    params.set_n_threads(threads());
    params.set_translate(false);
    params.set_language(Some(language.unwrap_or("auto")));
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_token_timestamps(true);
    params.set_suppress_blank(true);
    params.set_suppress_nst(true);
    // SAFETY: `cancelled` only reads the flag, and `cancel` keeps it alive until `full` has returned.
    unsafe {
        params.set_abort_callback(Some(cancelled));
        params.set_abort_callback_user_data(Arc::as_ptr(&cancel).cast_mut().cast::<c_void>());
    }
    let report = progress.clone();
    params.set_progress_callback_safe(move |percent: i32| report(f64::from(percent.clamp(0, 100)) / 100.0));
    let result = state.full(params, audio);
    if cancel.load(Ordering::Relaxed) {
        return Err(CaptionError::Cancelled);
    }
    result.map_err(failed)?;
    progress(1.0);

    let language = whisper_rs::get_lang_str(state.full_lang_id_from_state()).map(str::to_string);
    let special = context.token_eot();
    let mut words = Vec::new();
    for segment in state.as_iter() {
        if segment.no_speech_probability() > NO_SPEECH {
            continue;
        }
        let (from, to) = (segment.start_timestamp() as f64 / 100.0, segment.end_timestamp() as f64 / 100.0);
        // Tokens can split a character, so a word's text is decoded only once it's whole.
        let mut word: Option<Partial> = None;
        for i in 0..segment.n_tokens() {
            let Some(token) = segment.get_token(i) else { continue };
            if token.token_id() >= special {
                continue;
            }
            let Ok(bytes) = token.to_bytes() else { continue };
            let data = token.token_data();
            let (t0, t1) = (data.t0 as f64 / 100.0, data.t1 as f64 / 100.0);
            let spaced = bytes.starts_with(b" ");
            match &mut word {
                Some((text, _, end, _)) if !spaced && !breaks_before(text, bytes) => {
                    text.extend_from_slice(bytes);
                    *end = end.max(t1);
                }
                Some(_) => {
                    if let Some(whole) = word.take() {
                        push(&mut words, whole, from, to);
                    }
                    word = Some((bytes.to_vec(), t0, t1, Some(spaced)));
                }
                None => word = Some((bytes.to_vec(), t0, t1, spaced.then_some(true))),
            }
        }
        if let Some(whole) = word.take() {
            push(&mut words, whole, from, to);
        }
    }
    Ok(Transcript { words, language })
}

/// Adds a finished word, timed inside its segment (`from` to `to`), unless it's empty or a sound effect. A word that
/// starts a segment without a space gets one, unless it and the word before are both in a script without spaces.
fn push(words: &mut Vec<Word>, (bytes, start, end, spaced): Partial, from: f64, to: f64) {
    let text = String::from_utf8_lossy(&bytes).trim().to_string();
    if text.is_empty() || sound_effect(&text) {
        return;
    }
    let spaced = spaced.unwrap_or_else(|| {
        let after_unspaced = words.last().and_then(|w| w.text.chars().last()).is_some_and(unspaced);
        !(after_unspaced && text.chars().next().is_some_and(unspaced))
    });
    let start = if start.is_finite() && start >= 0.0 { start.clamp(from, to) } else { from };
    let end = if end.is_finite() && end >= 0.0 { end.clamp(start, to) } else { to };
    words.push(Word { start, end, text, spaced });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The samples of a WAV file of 32-bit floats, one channel, as `say` writes them.
    #[cfg(target_os = "macos")]
    fn wav_samples(path: &Path) -> Vec<f32> {
        let bytes = std::fs::read(path).unwrap();
        let mut at = 12;
        while at + 8 <= bytes.len() {
            let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
            if &bytes[at..at + 4] == b"data" {
                let data = &bytes[at + 8..(at + 8 + size).min(bytes.len())];
                return data.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect();
            }
            at += 8 + size + (size & 1);
        }
        panic!("no sound in {}", path.display());
    }

    /// Speech in the system's voice, transcribed with the downloaded model: the one `SHOTMAXXING_MODEL` names, or
    /// the app's own copy. The model is a 60 MB download, so this runs only when asked
    /// (`cargo test -p shotmaxxing-captions -- --ignored`).
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "needs the downloaded speech model"]
    fn transcribes_speech_and_stops_when_cancelled() {
        let model =
            std::env::var_os("SHOTMAXXING_MODEL").map(std::path::PathBuf::from).unwrap_or_else(|| {
                let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
                crate::model_path(&home.join("Library/Application Support/dev.shotmaxxing.desktop/models"))
            });
        assert!(model.is_file(), "no model at {}", model.display());
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("speech.wav");
        let said = std::process::Command::new("say")
            .arg("--data-format=LEF32@16000")
            .arg("-o")
            .arg(&wav)
            .arg("Start with Get started, at the top right of the page. Then type your work email.")
            .status()
            .unwrap();
        assert!(said.success());
        let samples = wav_samples(&wav);
        let seconds = samples.len() as f64 / f64::from(SAMPLE_RATE);

        let heard =
            transcribe(&model, &samples, None, Arc::new(|_| {}), Arc::new(AtomicBool::new(false))).unwrap();
        let text = heard.words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ").to_lowercase();
        for said in ["get started", "top right", "work email"] {
            assert!(text.contains(said), "{said:?} in {text:?}");
        }
        assert_eq!(heard.language.as_deref(), Some("en"));
        assert!(
            heard.words.windows(2).all(|w| w[0].start <= w[1].start)
                && heard.words.iter().all(|w| w.end <= seconds + 0.5),
            "words in order, inside the {seconds:.1} s of sound: {:?}",
            heard.words
        );

        let stopped = transcribe(&model, &samples, None, Arc::new(|_| {}), Arc::new(AtomicBool::new(true)));
        assert!(matches!(stopped, Err(CaptionError::Cancelled)), "{stopped:?}");

        // After two seconds of silence, the first words are timed where they're said, not from the start.
        let mut late = vec![0.0; 2 * SAMPLE_RATE as usize];
        late.extend_from_slice(&samples);
        let quiet = [(0.0, 2.0)];
        let heard = crate::transcribe(
            &model,
            &late,
            &quiet,
            None,
            Arc::new(|_| {}),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let first = &heard.words[0];
        assert!(first.text.to_lowercase().starts_with("start") && first.start >= 1.6, "{:?}", heard.words);
    }

    #[test]
    fn sound_effects_are_not_words() {
        for effect in ["[BLANK_AUDIO]", "(upbeat music)", "*laughs*", "♪", "♪♪", "[Music]"] {
            assert!(sound_effect(effect), "{effect}");
        }
        for word in ["Hello", "(but", "it's", "[x", "C++"] {
            assert!(!sound_effect(word), "{word}");
        }
    }

    #[test]
    fn words_stay_inside_their_segment() {
        let mut words = Vec::new();
        push(&mut words, (b" Hello".to_vec(), 0.5, 9.0, Some(true)), 1.0, 2.0);
        push(&mut words, (b"  ".to_vec(), 1.2, 1.3, Some(true)), 1.0, 2.0);
        push(&mut words, (b" [Music]".to_vec(), 1.2, 1.3, Some(true)), 1.0, 2.0);
        push(&mut words, (b" world".to_vec(), -0.01, -0.01, Some(true)), 1.0, 2.0);
        assert_eq!(
            words,
            [
                Word { start: 1.0, end: 2.0, text: "Hello".into(), spaced: true },
                Word { start: 1.0, end: 2.0, text: "world".into(), spaced: true }
            ]
        );
    }

    #[test]
    fn chinese_and_japanese_split_between_whole_characters() {
        let (kan, ji) = ("漢".as_bytes(), "字".as_bytes());
        assert!(breaks_before(kan, ji));
        assert!(breaks_before("す。".as_bytes(), "次".as_bytes()), "after their full stop");
        assert!(!breaks_before(kan, "。".as_bytes()), "never before punctuation");
        assert!(!breaks_before(kan, &ji[..1]), "never inside a character");
        assert!(!breaks_before(b"Hello", ji), "a new word after Latin needs its space");
        assert!(!breaks_before("한".as_bytes(), "국".as_bytes()), "Korean uses spaces");

        // A new segment joins the last one without a space only between two unspaced scripts.
        let mut words = Vec::new();
        push(&mut words, ("保存".as_bytes().to_vec(), 0.0, 0.5, None), 0.0, 1.0);
        push(&mut words, ("します".as_bytes().to_vec(), 1.0, 1.5, None), 1.0, 2.0);
        push(&mut words, (b"OK".to_vec(), 2.0, 2.5, None), 2.0, 3.0);
        assert_eq!(words.iter().map(|w| w.spaced).collect::<Vec<_>>(), [true, false, true]);
    }
}
