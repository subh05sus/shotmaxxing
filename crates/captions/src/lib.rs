//! On-device captions: speech transcribed with Whisper (whisper.cpp, on the GPU through Metal on macOS), and the
//! words grouped into captions that are easy to read.
//!
//! Nothing here sends sound anywhere. The one network request is the speech model's download, made once and only
//! when someone asks for captions, from a fixed commit on Hugging Face, and the file is used only if its SHA-256
//! matches the one pinned here.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use shotmaxxing_core::captions::{FILE_ROW, width};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(any(target_os = "macos", windows))]
mod whisper;
#[cfg(windows)]
mod win;

/// Whisper listens at 16 kHz, one channel.
pub const SAMPLE_RATE: u32 = 16_000;

/// A speech model file and where it comes from.
pub struct Model {
    pub file: &'static str,
    pub url: &'static str,
    /// Lowercase hex.
    pub sha256: &'static str,
    pub bytes: u64,
}

/// Whisper base, multilingual (99 languages), quantised to 5 bits: whisper.cpp's `ggml-base-q5_1.bin`, about 60 MB,
/// from a fixed commit so the file can't change under its hash.
pub const MODEL: Model = Model {
    file: "ggml-base-q5_1.bin",
    url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-base-q5_1.bin",
    sha256: "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898",
    bytes: 59_707_625,
};

#[derive(Debug, thiserror::Error)]
pub enum CaptionError {
    #[error("cancelled")]
    Cancelled,
    #[error("couldn't download the speech model: {0}")]
    Download(String),
    #[error("the speech model arrived damaged, so it wasn't used. Try again")]
    Damaged,
    #[error("couldn't transcribe: {0}")]
    Whisper(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("captions need macOS or Windows")]
    Unsupported,
    #[error("captions need a newer processor, one with AVX2 (most from 2013 on)")]
    OldProcessor,
}

/// Whether this computer can make captions: macOS or Windows, and on Intel and AMD processors, AVX2, which
/// whisper.cpp is built for (without it, transcribing would crash the app).
pub fn supported() -> Result<(), CaptionError> {
    if !cfg!(any(target_os = "macos", windows)) {
        return Err(CaptionError::Unsupported);
    }
    #[cfg(target_arch = "x86_64")]
    {
        let ready = std::arch::is_x86_feature_detected!("avx")
            && std::arch::is_x86_feature_detected!("avx2")
            && std::arch::is_x86_feature_detected!("fma")
            && std::arch::is_x86_feature_detected!("f16c")
            && std::arch::is_x86_feature_detected!("bmi2");
        if !ready {
            return Err(CaptionError::OldProcessor);
        }
    }
    Ok(())
}

/// Where the model lives in `dir`.
pub fn model_path(dir: &Path) -> PathBuf {
    dir.join(MODEL.file)
}

/// Whether the model is in `dir`, whole. Its hash was checked before it took its name, so a file of the right size
/// there is the one that was checked.
pub fn model_ready(dir: &Path) -> bool {
    std::fs::metadata(model_path(dir)).is_ok_and(|m| m.is_file() && m.len() == MODEL.bytes)
}

/// Downloads the model into `dir` unless it's there already. `progress` hears the bytes received and expected, and
/// cancels by returning false. The file arrives under a hidden name and takes its own only once its hash matches.
pub fn fetch_model(dir: &Path, progress: &mut dyn FnMut(u64, u64) -> bool) -> Result<PathBuf, CaptionError> {
    let target = model_path(dir);
    if model_ready(dir) {
        return Ok(target);
    }
    std::fs::create_dir_all(dir)?;
    let partial = dir.join(format!(".{}.partial", MODEL.file));
    let _ = std::fs::remove_file(&partial);
    let fetched = download(MODEL.url, &partial, MODEL.bytes, progress)
        .and_then(|()| if sha256(&partial)? == MODEL.sha256 { Ok(()) } else { Err(CaptionError::Damaged) });
    match fetched {
        Ok(()) => {
            std::fs::rename(&partial, &target)?;
            Ok(target)
        }
        Err(error) => {
            let _ = std::fs::remove_file(&partial);
            Err(error)
        }
    }
}

/// A file's SHA-256, in lowercase hex.
fn sha256(path: &Path) -> Result<String, std::io::Error> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    std::io::copy(&mut std::fs::File::open(path)?, &mut hasher)?;
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(target_os = "macos")]
fn download(
    url: &str,
    to: &Path,
    expected: u64,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<(), CaptionError> {
    macos::download(url, to, expected, progress)
}

#[cfg(windows)]
fn download(
    url: &str,
    to: &Path,
    expected: u64,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<(), CaptionError> {
    win::download(url, to, expected, progress)
}

#[cfg(not(any(target_os = "macos", windows)))]
fn download(_: &str, _: &Path, _: u64, _: &mut dyn FnMut(u64, u64) -> bool) -> Result<(), CaptionError> {
    Err(CaptionError::Unsupported)
}

/// A word Whisper heard, in seconds from the start of the sound it was given.
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub start: f64,
    pub end: f64,
    pub text: String,
    /// Whether a space comes before it. Chinese and Japanese put none between words.
    pub spaced: bool,
}

/// What a stretch of speech said.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Transcript {
    pub words: Vec<Word>,
    /// Whisper's code for the language it heard ("en").
    pub language: Option<String>,
}

/// Transcribes `audio`, 16 kHz mono speech, with the model at `model`; its words are timed in seconds of `audio`.
/// `quiet` holds its quiet stretches (seconds), which are squeezed out first ([`Squeezed`] says why). `language` is
/// Whisper's code for the language spoken ("en"), or `None` to have Whisper work it out. `progress` hears how far
/// along it is, from 0 to 1, and setting `cancel` stops it.
pub fn transcribe(
    model: &Path,
    audio: &[f32],
    quiet: &[(f64, f64)],
    language: Option<&str>,
    progress: Arc<dyn Fn(f64) + Send + Sync>,
    cancel: Arc<AtomicBool>,
) -> Result<Transcript, CaptionError> {
    #[cfg(any(target_os = "macos", windows))]
    {
        supported()?;
        let squeezed = Squeezed::new(audio, quiet);
        let mut transcript = whisper::transcribe(model, &squeezed.samples, language, progress, cancel)?;
        for word in &mut transcript.words {
            word.start = squeezed.voice_time(word.start, false);
            word.end = squeezed.voice_time(word.end, true).max(word.start);
        }
        Ok(transcript)
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (model, audio, quiet, language, progress, cancel);
        Err(CaptionError::Unsupported)
    }
}

/// How much of a quiet stretch stays next to the speech on each side, in seconds: a natural pause.
const QUIET_KEPT: f64 = 0.3;
/// The least silence worth squeezing out, in seconds.
const SQUEEZE_LEAST: f64 = 0.4;

/// A voice with its long silences squeezed out, as Whisper hears it. Whisper times the first words of a stretch of
/// speech from where its window begins, so words said after a long silence were timed inside it (in a test, "Here
/// is how" said at 2 s was timed from 0 s); and it can make up words in silence ("Thank you."). Each quiet stretch
/// keeps a third of a second next to the speech on either side, and the words' times are moved back afterwards.
struct Squeezed {
    samples: Vec<f32>,
    /// Where each piece kept starts, in the squeezed sound and in the voice, in seconds, in order.
    pieces: Vec<(f64, f64)>,
}

impl Squeezed {
    fn new(audio: &[f32], quiet: &[(f64, f64)]) -> Self {
        let rate = f64::from(SAMPLE_RATE);
        let total = audio.len() as f64 / rate;
        // What's left out: each quiet stretch but what's next to speech (all of it before the first word and after the
        // last, but the part next to that word).
        let mut cuts: Vec<(f64, f64)> = quiet
            .iter()
            .map(|&(a, b)| {
                let from = if a <= 0.0 { 0.0 } else { a + QUIET_KEPT };
                let to = if b >= total - 0.05 { total } else { b - QUIET_KEPT };
                (from, to.min(total))
            })
            .filter(|(from, to)| to - from >= SQUEEZE_LEAST)
            .collect();
        cuts.sort_by(|x, y| x.0.total_cmp(&y.0));
        let sample = |t: f64| ((t * rate).round().max(0.0) as usize).min(audio.len());
        let (mut samples, mut pieces, mut at) = (Vec::with_capacity(audio.len()), Vec::new(), 0.0);
        for (from, to) in cuts.into_iter().chain(std::iter::once((total, total))) {
            if from > at {
                pieces.push((samples.len() as f64 / rate, at));
                samples.extend_from_slice(&audio[sample(at)..sample(from).max(sample(at))]);
            }
            at = f64::max(at, to);
        }
        Self { samples, pieces }
    }

    /// Where moment `t` of the squeezed sound is in the voice. An `end` exactly where two pieces meet stays with the
    /// piece before, so a word never stretches across the silence left out.
    fn voice_time(&self, t: f64, end: bool) -> f64 {
        let piece = self.pieces.iter().rev().find(|(start, _)| if end { *start < t } else { *start <= t });
        piece.or(self.pieces.first()).map_or(t, |(start, voice)| voice + (t - start))
    }
}

/// A caption: a few seconds of words, in seconds from the start of the sound.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// The most a caption holds on a wide frame: two rows of a subtitle file.
pub const MOST_UNITS: usize = 2 * FILE_ROW;
/// The longest a caption stays up.
const LONGEST: f64 = 6.0;
/// A pause this long between words starts a new caption.
const PAUSE: f64 = 0.7;
/// Captions shorter than this end at a full stop only when the next sentence follows: "Okay." stays with what comes
/// after it.
const SHORT_SENTENCE: usize = 12;
/// A caption this full (as a share of the most it holds) ends at a comma or a semicolon.
const FULL_ENOUGH: f64 = 0.6;
/// The least time a caption stays up, when the next one leaves time, so it can be read.
const SHORTEST: f64 = 1.0;
/// How long a caption lingers after its last word, when the next one leaves time.
const LINGER: f64 = 0.4;

fn ends_with_any(text: &str, marks: &[char]) -> bool {
    text.trim_end_matches(['"', '\'', '”', '’', ')', ']', '»', '」', '』'])
        .ends_with(|c: char| marks.contains(&c))
}

fn joined(words: &[&Word]) -> String {
    let mut text = String::new();
    for (i, word) in words.iter().enumerate() {
        if i > 0 && word.spaced {
            text.push(' ');
        }
        text.push_str(&word.text);
    }
    text
}

/// `words` grouped into captions: a new caption starts after a sentence ends, at a pause, before one would hold
/// more than `most` units (two rows of the frame it's for: see `row_units`) or stay up more than six seconds, and at
/// a comma once one is well filled. Each then stays up a little after its last word, and at least a second, as far
/// as the next caption allows.
pub fn lines(words: &[Word], most: usize) -> Vec<Line> {
    let most = most.clamp(SHORT_SENTENCE * 2, MOST_UNITS);
    let full_enough = (most as f64 * FULL_ENOUGH).round() as usize;
    let mut groups: Vec<Vec<&Word>> = Vec::new();
    let mut current: Vec<&Word> = Vec::new();
    for word in words {
        if let (Some(first), Some(last)) = (current.first(), current.last()) {
            let so_far = width(&joined(&current));
            let full = so_far + usize::from(word.spaced) + width(&word.text) > most;
            let long = word.end - first.start > LONGEST;
            let paused = word.start - last.end >= PAUSE;
            let sentence = so_far >= SHORT_SENTENCE
                && ends_with_any(&last.text, &['.', '!', '?', '…', '。', '！', '？']);
            let clause =
                so_far >= full_enough && ends_with_any(&last.text, &[',', ';', ':', '，', '；', '：', '、']);
            if full || long || paused || sentence || clause {
                groups.push(std::mem::take(&mut current));
            }
        }
        current.push(word);
    }
    if !current.is_empty() {
        groups.push(current);
    }
    let mut out: Vec<Line> = groups
        .iter()
        .map(|g| Line { start: g[0].start, end: g[g.len() - 1].end.max(g[0].start), text: joined(g) })
        .collect();
    for i in 0..out.len() {
        let next = out.get(i + 1).map_or(f64::INFINITY, |n| n.start);
        let wanted = (out[i].end + LINGER).max(out[i].start + SHORTEST);
        out[i].end = wanted.min(next).max(out[i].end);
    }
    out
}

#[cfg(test)]
mod tests {
    use shotmaxxing_core::project::CaptionStyle;

    use super::*;

    fn lines_wide(words: &[Word]) -> Vec<Line> {
        lines(words, MOST_UNITS)
    }

    fn words(spec: &[(f64, f64, &str)]) -> Vec<Word> {
        spec.iter().map(|&(start, end, text)| Word { start, end, text: text.into(), spaced: true }).collect()
    }

    #[test]
    fn sentences_become_captions_that_linger_and_never_overlap() {
        let out = lines_wide(&words(&[
            (0.4, 0.6, "Here's"),
            (0.6, 0.8, "how"),
            (0.8, 1.2, "sign-up"),
            (1.2, 1.6, "works."),
            (1.7, 2.0, "Start"),
            (2.0, 2.2, "with"),
            (2.2, 2.6, "Get"),
            (2.6, 3.0, "started."),
        ]));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, "Here's how sign-up works.");
        assert_eq!(out[1].text, "Start with Get started.");
        assert_eq!(out[0].end, 1.7, "it lingers until the next one starts");
        assert!((out[1].end - 3.4).abs() < 1e-9, "the last lingers 0.4 s");
    }

    #[test]
    fn a_short_sentence_stays_with_the_next_and_pauses_split() {
        let out = lines_wide(&words(&[
            (0.0, 0.3, "Okay."),
            (0.4, 0.6, "So"),
            (0.6, 0.9, "let's"),
            (0.9, 1.2, "start"),
            (3.0, 3.2, "Now"),
            (3.2, 3.5, "pricing"),
        ]));
        assert_eq!(
            out.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
            ["Okay. So let's start", "Now pricing"]
        );
        assert!((out[1].end - 4.0).abs() < 1e-9, "stays up for at least a second");
    }

    #[test]
    fn a_long_run_of_words_splits_before_two_rows_or_six_seconds() {
        let spec: Vec<(f64, f64, String)> =
            (0..40).map(|i| (i as f64 * 0.3, i as f64 * 0.3 + 0.25, format!("word{i}"))).collect();
        let input: Vec<Word> =
            spec.iter().map(|(s, e, t)| Word { start: *s, end: *e, text: t.clone(), spaced: true }).collect();
        let out = lines_wide(&input);
        assert!(out.len() > 2);
        for line in &out {
            assert!(width(&line.text) <= MOST_UNITS, "{}", line.text);
            assert!(line.end - line.start <= LONGEST + LINGER + 1e-9);
        }
        for pair in out.windows(2) {
            assert!(pair[0].end <= pair[1].start);
        }
        let all: Vec<String> = out.iter().flat_map(|l| l.text.split(' ').map(String::from)).collect();
        assert_eq!(all.len(), 40, "no word lost or repeated");
    }

    #[test]
    fn a_full_caption_ends_at_a_comma() {
        let text = "So when you open the settings page for the first time, you will see the list";
        let input: Vec<Word> = text
            .split(' ')
            .enumerate()
            .map(|(i, t)| Word {
                start: i as f64 * 0.2,
                end: i as f64 * 0.2 + 0.15,
                text: t.into(),
                spaced: true,
            })
            .collect();
        let out = lines_wide(&input);
        assert_eq!(out[0].text, "So when you open the settings page for the first time,");
        assert_eq!(out[1].text, "you will see the list");
    }

    #[test]
    fn a_tall_frame_gets_shorter_captions() {
        let text = "So when you open the settings page for the first time you will see the list of projects";
        let input: Vec<Word> = text
            .split(' ')
            .enumerate()
            .map(|(i, t)| Word {
                start: i as f64 * 0.2,
                end: i as f64 * 0.2 + 0.15,
                text: t.into(),
                spaced: true,
            })
            .collect();
        let tall = lines(&input, 2 * shotmaxxing_core::captions::row_units(9.0 / 16.0, CaptionStyle::Bold));
        assert!(tall.len() > lines_wide(&input).len());
        assert!(tall.iter().all(|l| width(&l.text) <= 32), "{tall:?}");
        // Never so short that a word or two makes a caption, whatever's asked.
        assert!(lines(&input, 3).iter().all(|l| l.text.contains(' ')));
    }

    #[test]
    fn chinese_and_japanese_join_without_spaces_and_still_split() {
        // Forty characters of Japanese, a word each fifth of a second, with a full stop after the twentieth.
        let text: Vec<char> =
            "設定ページを開くと最初に一覧が表示されます。次にパスワードを入力して保存ボタンを押してく"
                .chars()
                .collect();
        let input: Vec<Word> = text
            .iter()
            .enumerate()
            .map(|(i, c)| Word {
                start: i as f64 * 0.2,
                end: i as f64 * 0.2 + 0.15,
                text: c.to_string(),
                spaced: false,
            })
            .collect();
        let out = lines_wide(&input);
        assert_eq!(out[0].text, "設定ページを開くと最初に一覧が表示されます。");
        assert!(out.iter().all(|l| !l.text.contains(' ')));
        assert_eq!(out.iter().map(|l| l.text.as_str()).collect::<String>(), text.iter().collect::<String>());
        for line in &out {
            assert!(width(&line.text) <= MOST_UNITS && line.end - line.start <= LONGEST + LINGER + 1e-9);
        }
    }

    #[test]
    fn long_silences_are_squeezed_out_and_times_moved_back() {
        let rate = SAMPLE_RATE as usize;
        let audio: Vec<f32> = (0..10 * rate).map(|i| i as f32).collect();
        // Quiet until 2 s, speech to 5 s, quiet to 9 s, speech to the end with a short pause at 9.6 s.
        let squeezed = Squeezed::new(&audio, &[(0.0, 2.0), (5.0, 9.0), (9.6, 9.8)]);
        // Kept: 1.7 to 5.3 s and 8.7 s to the end; the short pause stays.
        assert_eq!(squeezed.samples.len(), 36 * rate / 10 + 13 * rate / 10);
        assert_eq!(squeezed.samples[0], (17 * rate / 10) as f32);
        assert_eq!(squeezed.samples[36 * rate / 10], (87 * rate / 10) as f32);
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(near(squeezed.voice_time(0.0, false), 1.7));
        assert!(near(squeezed.voice_time(3.6, false), 8.7), "a word starting there is in the next piece");
        assert!(near(squeezed.voice_time(3.6, true), 5.3), "one ending there stays with the speech before");
        assert!(near(squeezed.voice_time(4.0, false), 9.1));
        assert!(
            near(squeezed.voice_time(5.2, true), 10.3),
            "a time past the end runs on from the last piece"
        );

        // A voice without a clear pause is heard whole.
        let whole = Squeezed::new(&audio, &[]);
        assert_eq!(whole.samples.len(), audio.len());
        assert!(near(whole.voice_time(4.2, false), 4.2));
        // All quiet: nothing to hear.
        assert!(Squeezed::new(&audio, &[(0.0, 10.0)]).samples.is_empty());
        assert!(lines_wide(&[]).is_empty());
    }

    #[test]
    fn the_model_is_ready_only_when_whole() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!model_ready(dir.path()));
        std::fs::write(model_path(dir.path()), b"not a model").unwrap();
        assert!(!model_ready(dir.path()), "the wrong size is a damaged file");
        let file = std::fs::File::create(model_path(dir.path())).unwrap();
        file.set_len(MODEL.bytes).unwrap();
        assert!(model_ready(dir.path()));
    }

    #[test]
    fn hashes_are_lowercase_hex() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("abc");
        std::fs::write(&file, b"abc").unwrap();
        assert_eq!(
            sha256(&file).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
