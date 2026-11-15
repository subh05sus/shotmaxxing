//! Caption text: how a caption breaks into rows on screen, and the subtitle files (SRT, WebVTT) it's saved as.
//!
//! The editor's preview breaks rows with the same rules (`apps/desktop/src/lib/captions.ts`), so a caption looks the
//! same there and in the export. Change both together.

use unicode_segmentation::UnicodeSegmentation;

use crate::project::CaptionStyle;

/// The most a subtitle file's row holds, the broadcast convention.
pub const FILE_ROW: usize = 42;

/// How wide `c` is: East Asian wide and full-width characters, and emoji, take two units; everything else one.
pub fn units(c: char) -> usize {
    match u32::from(c) {
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x2FFFD
        | 0x30000..=0x3FFFD => 2,
        _ => 1,
    }
}

/// How wide a grapheme (what reads as one character: a letter and its accents, an emoji and its modifiers) is: as
/// wide as its first character, since the marks after it take no room of their own.
fn cluster_units(grapheme: &str) -> usize {
    grapheme.chars().next().map_or(0, units)
}

/// How wide `text` is, in [`units`].
pub fn width(text: &str) -> usize {
    text.graphemes(true).map(cluster_units).sum()
}

/// The most units a row holds on a frame `aspect` wide (width ÷ height) in `style`: 80% of the frame at about 0.55
/// em a unit, from 14 to 42.
pub fn row_units(aspect: f64, style: CaptionStyle) -> usize {
    let fit = 0.8 * aspect / (0.55 * style.size());
    if fit.is_finite() && fit > 0.0 { (fit.floor() as usize).clamp(14, FILE_ROW) } else { FILE_ROW }
}

/// A word, or a piece of one too wide for a row, and whether a space comes before it.
struct Piece {
    text: String,
    units: usize,
    spaced: bool,
}

/// The words of `text`, with any word wider than `max` cut into even pieces between graphemes (text without spaces,
/// like Chinese or Japanese, is one long word).
fn pieces(text: &str, max: usize) -> Vec<Piece> {
    let mut out = Vec::new();
    for word in text.split_whitespace() {
        let total = width(word);
        if total <= max {
            out.push(Piece { text: word.to_string(), units: total, spaced: !out.is_empty() });
            continue;
        }
        let parts = total.div_ceil(max);
        let target = total.div_ceil(parts);
        let mut piece = Piece { text: String::new(), units: 0, spaced: !out.is_empty() };
        for grapheme in word.graphemes(true) {
            let u = cluster_units(grapheme);
            if piece.units > 0 && piece.units + u > target {
                out.push(piece);
                piece = Piece { text: String::new(), units: 0, spaced: false };
            }
            piece.text.push_str(grapheme);
            piece.units += u;
        }
        if piece.units > 0 {
            out.push(piece);
        }
    }
    out
}

fn joined(pieces: &[Piece]) -> (String, usize) {
    let mut text = String::new();
    let mut units = 0;
    for (i, piece) in pieces.iter().enumerate() {
        if i > 0 && piece.spaced {
            text.push(' ');
            units += 1;
        }
        text.push_str(&piece.text);
        units += piece.units;
    }
    (text, units)
}

/// `list` broken into rows of at most `max` units: one row when it fits, two rows as even as they can be when two
/// are enough, and full rows after that.
fn rows_of(list: &[Piece], max: usize) -> Vec<String> {
    if list.is_empty() {
        return Vec::new();
    }
    let (all, total) = joined(list);
    if total <= max {
        return vec![all];
    }
    if total <= 2 * max {
        let mut best: Option<(usize, usize)> = None;
        for i in 1..list.len() {
            let (a, b) = (joined(&list[..i]).1, joined(&list[i..]).1);
            if a <= max && b <= max && best.is_none_or(|(longest, _)| a.max(b) < longest) {
                best = Some((a.max(b), i));
            }
        }
        if let Some((_, i)) = best {
            return vec![joined(&list[..i]).0, joined(&list[i..]).0];
        }
    }
    let mut rows = Vec::new();
    let mut start = 0;
    for i in 1..=list.len() {
        if i == list.len() || joined(&list[start..=i]).1 > max {
            rows.push(joined(&list[start..i]).0);
            start = i;
        }
    }
    rows
}

/// `text` broken into rows of at most `max` units: one row when it fits, two rows as even as they can be when two
/// are enough, and full rows after that. Words stay whole unless one is wider than a row.
pub fn rows(text: &str, max: usize) -> Vec<String> {
    let max = max.max(1);
    rows_of(&pieces(text, max), max)
}

/// How a caption shows on screen with rows of at most `max` units: pages of one or two rows, one after another. A
/// caption that fits in two rows is one page; a longer one is split between words into as few pages as will do, as
/// even as they can be, so a long caption never covers the picture.
pub fn pages(text: &str, max: usize) -> Vec<Vec<String>> {
    let max = max.max(1);
    let list = pieces(text, max);
    // best[j]: the fewest pages for the first j pieces, the widest of them, and where the last one starts.
    let mut best: Vec<Option<(usize, usize, usize)>> = vec![None; list.len() + 1];
    best[0] = Some((0, 0, 0));
    for j in 1..=list.len() {
        for i in (0..j).rev() {
            let wide = joined(&list[i..j]).1;
            if wide > 2 * max {
                break;
            }
            let Some((count, widest, _)) = best[i] else { continue };
            if rows_of(&list[i..j], max).len() > 2 {
                continue;
            }
            let candidate = (count + 1, widest.max(wide), i);
            if best[j].is_none_or(|(c, w, _)| (candidate.0, candidate.1) < (c, w)) {
                best[j] = Some(candidate);
            }
        }
    }
    let mut out = Vec::new();
    let mut j = list.len();
    while j > 0 {
        let Some((_, _, i)) = best[j] else { break };
        out.push(rows_of(&list[i..j], max));
        j = i;
    }
    out.reverse();
    out
}

/// Where each of `pages` stops showing, as a fraction of its caption's time: each shows for its share of the
/// caption's width, as the words take about that long to say.
pub fn page_ends(pages: &[Vec<String>]) -> Vec<f64> {
    let widths: Vec<usize> =
        pages.iter().map(|rows| rows.iter().map(|r| width(r)).sum::<usize>().max(1)).collect();
    let total = widths.iter().sum::<usize>().max(1) as f64;
    let mut at = 0;
    widths
        .iter()
        .enumerate()
        .map(|(i, w)| {
            at += w;
            if i + 1 == widths.len() { 1.0 } else { at as f64 / total }
        })
        .collect()
}

/// Which of `pages` shows `fraction` (0 to 1) of the way through its caption.
pub fn page_at(pages: &[Vec<String>], fraction: f64) -> usize {
    let ends = page_ends(pages);
    ends.iter().position(|end| fraction < *end).unwrap_or(ends.len().saturating_sub(1))
}

/// A caption on the finished video's timeline, in seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// `seconds` as `hh:mm:ss` plus milliseconds after `separator`.
fn stamp(seconds: f64, separator: char) -> String {
    let ms = (seconds.max(0.0) * 1000.0).round() as u64;
    let (h, m, s, ms) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    format!("{h:02}:{m:02}:{s:02}{separator}{ms:03}")
}

/// The cues worth writing: some text, and some time to read it. A cue's text is broken into rows here unless it
/// comes broken already, one row a line.
fn written(cues: &[Cue]) -> impl Iterator<Item = (&Cue, Vec<String>)> {
    cues.iter()
        .filter(|cue| cue.end > cue.start)
        .map(|cue| {
            let rows = if cue.text.contains('\n') {
                cue.text.lines().map(str::trim).filter(|r| !r.is_empty()).map(String::from).collect()
            } else {
                rows(&cue.text, FILE_ROW)
            };
            (cue, rows)
        })
        .filter(|(_, rows)| !rows.is_empty())
}

/// A caption at `start` to `end` seconds as subtitle cues: one, or one a page when it's too long for two rows of a
/// subtitle file, each for its share of the time.
pub fn cues_of(start: f64, end: f64, text: &str) -> Vec<Cue> {
    let pages = pages(text, FILE_ROW);
    let ends = page_ends(&pages);
    let mut from = start;
    pages
        .iter()
        .zip(ends)
        .map(|(rows, share)| {
            let to = if share >= 1.0 { end } else { start + (end - start) * share };
            let cue = Cue { start: from, end: to, text: rows.join("\n") };
            from = to;
            cue
        })
        .collect()
}

/// `cues` as a SubRip (`.srt`) file.
pub fn srt(cues: &[Cue]) -> String {
    let mut out = String::new();
    for (i, (cue, rows)) in written(cues).enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            stamp(cue.start, ','),
            stamp(cue.end, ','),
            rows.join("\n")
        ));
    }
    out
}

/// `cues` as a WebVTT (`.vtt`) file.
pub fn vtt(cues: &[Cue]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for (cue, rows) in written(cues) {
        // A line of its own that reads "-->" would end the cue early.
        let rows: Vec<String> = rows.into_iter().map(|r| r.replace("-->", "->")).collect();
        out.push_str(&format!(
            "{} --> {}\n{}\n\n",
            stamp(cue.start, '.'),
            stamp(cue.end, '.'),
            rows.join("\n")
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_one_row_and_spacing_is_tidied() {
        assert_eq!(rows("  Here's how   sign-up works. ", 42), ["Here's how sign-up works."]);
        assert!(rows("   ", 42).is_empty());
    }

    #[test]
    fn two_rows_come_out_as_even_as_the_words_allow() {
        let text = "Start with Get started, at the top right of the page.";
        assert_eq!(rows(text, 42), ["Start with Get started, at", "the top right of the page."]);
        for row in rows(text, 42) {
            assert!(width(&row) <= 42);
        }
    }

    #[test]
    fn longer_text_fills_rows_in_order() {
        let text = "one two three four five six seven eight nine ten eleven twelve";
        let out = rows(text, 14);
        assert_eq!(out, ["one two three", "four five six", "seven eight", "nine ten", "eleven twelve"]);
        assert!(out.iter().all(|r| width(r) <= 14));
    }

    #[test]
    fn text_without_spaces_breaks_evenly_between_characters() {
        // Twenty-five wide characters, fifty units: two rows of about twenty-five.
        let text: String = std::iter::repeat_n('字', 25).collect();
        let out = rows(&text, 42);
        assert_eq!(out.len(), 2);
        assert_eq!(out.concat(), text, "nothing added between the pieces");
        assert!(out.iter().all(|r| width(r) <= 42));
        assert_eq!(width("日本"), 4);
        let long = "a".repeat(100);
        let out = rows(&long, 42);
        assert_eq!(out.concat(), long);
        assert!(out.iter().all(|r| width(r) <= 42));
    }

    #[test]
    fn rows_break_between_graphemes_never_inside_one() {
        // Thai has no spaces and marks above and below its letters; a cut before a mark would orphan it.
        let thai = "ที่นี่คือหน้าตั้งค่าของคุณซึ่งคุณสามารถเปลี่ยนรหัสผ่านได้ทุกเมื่อที่ต้องการ";
        let out = rows(thai, 20);
        // The editor's preview (captions.ts) breaks it in the same places.
        assert_eq!(out, ["ที่นี่คือหน้าตั้งค่าของคุณซึ่", "งคุณสามารถเปลี่ยนรหัส", "ผ่านได้ทุกเมื่อที่ต้องการ"]);
        assert_eq!(out.concat(), thai);
        for row in &out {
            let first = row.graphemes(true).next().unwrap();
            assert!(thai.graphemes(true).any(|g| g == first), "{row} starts inside a grapheme");
            assert!(width(row) <= 20, "{row}");
        }
        // An accent written as its own character takes no room.
        assert_eq!(width("e\u{301}te\u{301}"), 3);
        // A family emoji is one picture, two units wide.
        assert_eq!(width("👨‍👩‍👧"), 2);
        let accents = "e\u{301}".repeat(30);
        assert!(rows(&accents, 14).iter().all(|r| r.starts_with('e')));
    }

    #[test]
    fn a_long_caption_shows_in_even_pages_of_two_rows() {
        let text = "Then enter your work email, and pick a password you'll remember for next time.";
        assert_eq!(
            pages(text, 42),
            [vec![
                "Then enter your work email, and pick a".to_string(),
                "password you'll remember for next time.".into()
            ]],
            "two rows of a wide frame hold it"
        );
        let out = pages(text, 21);
        let page = |rows: &[&str]| rows.iter().map(|r| r.to_string()).collect::<Vec<_>>();
        // The editor's preview (captions.ts) makes the same pages.
        assert_eq!(
            out,
            [
                page(&["Then enter your", "work email,"]),
                page(&["and pick a", "password you'll"]),
                page(&["remember for", "next time."])
            ]
        );
        assert_eq!(
            pages("日本語の字幕もきれいに二行に分かれます。", 16),
            [page(&["日本語の字幕も"]), page(&["きれいに二行に", "分かれます。"])]
        );
        for page in &out {
            assert!(page.len() <= 2, "{page:?}");
            assert!(page.iter().all(|r| width(r) <= 21), "{page:?}");
        }
        let words: Vec<String> = out.iter().flatten().flat_map(|r| r.split(' ').map(String::from)).collect();
        assert_eq!(words.join(" "), text, "every word, in order");
        assert_eq!(out.len(), 3, "as few pages as will do");
        assert!(
            out.iter().all(|p| p.iter().map(|r| width(r)).sum::<usize>() >= 15),
            "no page left nearly empty: {out:?}"
        );
        assert_eq!(pages("Short enough.", 21), [vec!["Short enough.".to_string()]]);
        assert!(pages("  ", 21).is_empty());

        let ends = page_ends(&out);
        assert_eq!(ends.len(), 3);
        assert_eq!(*ends.last().unwrap(), 1.0);
        assert!(ends.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(page_at(&out, 0.0), 0);
        assert_eq!(page_at(&out, 0.999), 2);
        assert_eq!(page_at(&out, 1.5), 2);
    }

    #[test]
    fn a_long_caption_becomes_cues_of_two_rows_at_most() {
        let text = "So when you open the settings page for the first time you will see the list of every project, and \
                    each one has its own button to share it with the people you work with";
        let cues = cues_of(10.0, 20.0, text);
        assert!(cues.len() >= 2);
        assert_eq!(cues.first().unwrap().start, 10.0);
        assert_eq!(cues.last().unwrap().end, 20.0);
        assert!(cues.windows(2).all(|w| w[0].end == w[1].start));
        let file = srt(&cues);
        for block in file.split("\n\n").filter(|b| !b.is_empty()) {
            let rows: Vec<&str> = block.lines().skip(2).collect();
            assert!(rows.len() <= 2 && rows.iter().all(|r| width(r) <= FILE_ROW), "{block}");
        }
        assert_eq!(cues_of(0.0, 1.0, "Hi."), [Cue { start: 0.0, end: 1.0, text: "Hi.".into() }]);
    }

    #[test]
    fn rows_are_narrower_on_narrow_frames() {
        assert_eq!(row_units(16.0 / 9.0, CaptionStyle::Bar), 42);
        assert_eq!(row_units(1.0, CaptionStyle::Bar), 38);
        assert_eq!(row_units(1.0, CaptionStyle::Bold), 29);
        assert_eq!(row_units(9.0 / 16.0, CaptionStyle::Bar), 21);
        assert_eq!(row_units(9.0 / 16.0, CaptionStyle::Bold), 16);
        assert_eq!(row_units(f64::NAN, CaptionStyle::Bar), 42);
    }

    #[test]
    fn subtitle_files_number_time_and_break_their_cues() {
        let cues = [
            Cue { start: 0.4, end: 2.8, text: "Here's how sign-up works.".into() },
            Cue { start: 3.0, end: 3.0, text: "Never shown".into() },
            Cue {
                start: 3661.5,
                end: 3664.0,
                text: "Then enter your work email, and pick a password you'll remember.".into(),
            },
            Cue { start: 3670.0, end: 3671.0, text: "  ".into() },
        ];
        assert_eq!(
            srt(&cues),
            "1\n00:00:00,400 --> 00:00:02,800\nHere's how sign-up works.\n\n\
             2\n01:01:01,500 --> 01:01:04,000\nThen enter your work email, and\npick a password you'll remember.\n\n"
        );
        let vtt = vtt(&cues);
        assert!(vtt.starts_with("WEBVTT\n\n00:00:00.400 --> 00:00:02.800\nHere's how sign-up works.\n\n"));
        assert!(vtt.contains("01:01:01.500 --> 01:01:04.000\n"));
        assert_eq!(
            super::vtt(&[Cue { start: 0.0, end: 1.0, text: "a --> b".into() }]),
            "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\na -> b\n\n"
        );
    }
}
