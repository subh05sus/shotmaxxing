//! What happened during a recording, besides the pixels.
//!
//! The real cursor is excluded from the video. Its position and shape are stored here instead, together with clicks,
//! keys, focus changes and screen-change regions. The editor redraws the cursor from this data and auto zoom reads it.
//!
//! Two files live in a project's `telemetry/` folder:
//! - `cursor.bin`: cursor samples in a compact delta + varint format (a few KB per minute),
//! - `events.jsonl`: one JSON event per line.

use crate::geometry::{Point, Rect};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum CursorShape {
    #[default]
    Arrow = 0,
    IBeam = 1,
    PointingHand = 2,
    OpenHand = 3,
    ClosedHand = 4,
    Crosshair = 5,
    ResizeHorizontal = 6,
    ResizeVertical = 7,
    NotAllowed = 8,
    Wait = 9,
    Hidden = 10,
    Other = 11,
}

impl CursorShape {
    fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Arrow,
            1 => Self::IBeam,
            2 => Self::PointingHand,
            3 => Self::OpenHand,
            4 => Self::ClosedHand,
            5 => Self::Crosshair,
            6 => Self::ResizeHorizontal,
            7 => Self::ResizeVertical,
            8 => Self::NotAllowed,
            9 => Self::Wait,
            10 => Self::Hidden,
            _ => Self::Other,
        }
    }
}

/// One cursor sample. Time is microseconds since the recording started; position is in capture-space points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CursorSample {
    pub t_us: u64,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub shape: CursorShape,
}

impl CursorSample {
    pub fn point(&self) -> Point {
        Point::new(self.x, self.y)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Where an input came from. Agent takes record their own provenance so edits can treat them differently.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InputSource {
    #[default]
    Person,
    Agent,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputEvent {
    MouseDown {
        t_us: u64,
        x: f64,
        y: f64,
        button: MouseButton,
        #[serde(default)]
        clicks: u8,
        #[serde(default)]
        source: InputSource,
    },
    MouseUp {
        t_us: u64,
        x: f64,
        y: f64,
        button: MouseButton,
        #[serde(default)]
        source: InputSource,
    },
    Scroll {
        t_us: u64,
        x: f64,
        y: f64,
        dx: f64,
        dy: f64,
    },
    /// A key press. `label` is what the keystroke overlay shows (`⌘S`, `a`, `Return`). Text typed into password
    /// fields is never recorded: those presses arrive with `redacted` set and no label.
    KeyDown {
        t_us: u64,
        label: Option<String>,
        #[serde(default)]
        modifiers_only: bool,
        #[serde(default)]
        redacted: bool,
        #[serde(default)]
        source: InputSource,
    },
    /// The focused control changed (from the accessibility tree).
    Focus {
        t_us: u64,
        bounds: Rect,
        role: String,
        label: Option<String>,
    },
    /// Part of the screen changed. `fraction` is the changed share of the capture area.
    ScreenChange {
        t_us: u64,
        region: Rect,
        fraction: f64,
    },
    /// A caption from an agent's `say` beat.
    Caption {
        t_us: u64,
        end_us: u64,
        text: String,
    },
}

impl InputEvent {
    pub fn t_us(&self) -> u64 {
        match self {
            Self::MouseDown { t_us, .. }
            | Self::MouseUp { t_us, .. }
            | Self::Scroll { t_us, .. }
            | Self::KeyDown { t_us, .. }
            | Self::Focus { t_us, .. }
            | Self::ScreenChange { t_us, .. }
            | Self::Caption { t_us, .. } => *t_us,
        }
    }
}

#[derive(Debug, Error)]
pub enum TelemetryError {
    #[error("telemetry file is not a Shotmaxxing cursor file")]
    BadMagic,
    #[error("cursor file version {0} is newer than this app understands")]
    UnsupportedVersion(u8),
    #[error("cursor file is truncated or damaged")]
    Truncated,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("event line {line}: {source}")]
    Event { line: usize, source: serde_json::Error },
}

const MAGIC: &[u8; 4] = b"SMXC";
const VERSION: u8 = 1;
/// Positions are stored in eighths of a point.
const QUANT: f64 = 8.0;

/// Encodes cursor samples into the compact `cursor.bin` format.
///
/// Layout: `SMXC`, version byte, varint sample count, then per sample: varint `(dt_us << 1) | shape_changed`,
/// zigzag varint dx, zigzag varint dy (eighths of a point), and the shape byte when it changed.
pub fn encode_cursor(samples: &[CursorSample]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + samples.len() * 4);
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    write_varint(&mut out, samples.len() as u64);
    let (mut last_t, mut last_x, mut last_y) = (0u64, 0i64, 0i64);
    let mut last_shape: Option<CursorShape> = None;
    for s in samples {
        let t = s.t_us.max(last_t);
        let qx = (s.x * QUANT).round() as i64;
        let qy = (s.y * QUANT).round() as i64;
        let changed = last_shape != Some(s.shape);
        write_varint(&mut out, ((t - last_t) << 1) | u64::from(changed));
        write_varint(&mut out, zigzag(qx - last_x));
        write_varint(&mut out, zigzag(qy - last_y));
        if changed {
            out.push(s.shape as u8);
        }
        (last_t, last_x, last_y, last_shape) = (t, qx, qy, Some(s.shape));
    }
    out
}

pub fn decode_cursor(bytes: &[u8]) -> Result<Vec<CursorSample>, TelemetryError> {
    if bytes.len() < 5 || &bytes[..4] != MAGIC {
        return Err(TelemetryError::BadMagic);
    }
    if bytes[4] != VERSION {
        return Err(TelemetryError::UnsupportedVersion(bytes[4]));
    }
    let mut pos = 5;
    let count = read_varint(bytes, &mut pos)? as usize;
    let mut samples = Vec::with_capacity(count.min(1 << 24));
    let (mut t, mut x, mut y) = (0u64, 0i64, 0i64);
    let mut shape = CursorShape::Arrow;
    for _ in 0..count {
        let head = read_varint(bytes, &mut pos)?;
        t += head >> 1;
        x += unzigzag(read_varint(bytes, &mut pos)?);
        y += unzigzag(read_varint(bytes, &mut pos)?);
        if head & 1 == 1 {
            shape = CursorShape::from_u8(*bytes.get(pos).ok_or(TelemetryError::Truncated)?);
            pos += 1;
        }
        samples.push(CursorSample { t_us: t, x: x as f64 / QUANT, y: y as f64 / QUANT, shape });
    }
    Ok(samples)
}

pub fn write_events<W: Write>(mut w: W, events: &[InputEvent]) -> Result<(), TelemetryError> {
    for event in events {
        serde_json::to_writer(&mut w, event).map_err(io::Error::other)?;
        w.write_all(b"\n")?;
    }
    Ok(())
}

/// Reads `events.jsonl`. Blank lines are skipped; a damaged line reports its number.
pub fn read_events<R: BufRead>(r: R) -> Result<Vec<InputEvent>, TelemetryError> {
    let mut events = Vec::new();
    for (i, line) in r.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        events.push(
            serde_json::from_str(&line).map_err(|source| TelemetryError::Event { line: i + 1, source })?,
        );
    }
    events.sort_by_key(InputEvent::t_us);
    Ok(events)
}

/// Linear interpolation of the cursor position at a time. Samples must be sorted by time.
pub fn cursor_at(samples: &[CursorSample], t_us: u64) -> Option<Point> {
    let first = samples.first()?;
    if t_us <= first.t_us {
        return Some(first.point());
    }
    let idx = samples.partition_point(|s| s.t_us <= t_us);
    if idx >= samples.len() {
        return samples.last().map(CursorSample::point);
    }
    let (a, b) = (&samples[idx - 1], &samples[idx]);
    let span = (b.t_us - a.t_us) as f64;
    let f = if span > 0.0 { (t_us - a.t_us) as f64 / span } else { 1.0 };
    Some(a.point().lerp(b.point(), f))
}

fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

fn unzigzag(v: u64) -> i64 {
    ((v >> 1) as i64) ^ -((v & 1) as i64)
}

fn write_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn read_varint(bytes: &[u8], pos: &mut usize) -> Result<u64, TelemetryError> {
    let mut result = 0u64;
    let mut shift = 0;
    loop {
        let byte = *bytes.get(*pos).ok_or(TelemetryError::Truncated)?;
        *pos += 1;
        if shift >= 64 {
            return Err(TelemetryError::Truncated);
        }
        result |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(result);
        }
        shift += 7;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples() -> Vec<CursorSample> {
        (0..600)
            .map(|i| CursorSample {
                t_us: i * 8_333,
                x: 400.0 + (i as f64 * 0.05).sin() * 300.0,
                y: 300.0 + (i as f64 * 0.03).cos() * 200.0,
                shape: if i > 300 { CursorShape::PointingHand } else { CursorShape::Arrow },
            })
            .collect()
    }

    #[test]
    fn cursor_round_trip_is_accurate_and_compact() {
        let input = samples();
        let bytes = encode_cursor(&input);
        let output = decode_cursor(&bytes).unwrap();
        assert_eq!(input.len(), output.len());
        for (a, b) in input.iter().zip(&output) {
            assert_eq!(a.t_us, b.t_us);
            assert!((a.x - b.x).abs() <= 1.0 / 16.0 + 1e-9);
            assert!((a.y - b.y).abs() <= 1.0 / 16.0 + 1e-9);
            assert_eq!(a.shape, b.shape);
        }
        // Five seconds of 120 Hz cursor data in well under 6 KB.
        assert!(bytes.len() < 6 * 1024, "{} bytes", bytes.len());
    }

    #[test]
    fn damaged_files_fail_cleanly() {
        let bytes = encode_cursor(&samples());
        assert!(matches!(decode_cursor(&bytes[..bytes.len() - 3]), Err(TelemetryError::Truncated)));
        assert!(matches!(decode_cursor(b"NOPE\x01"), Err(TelemetryError::BadMagic)));
        let mut future = bytes.clone();
        future[4] = 9;
        assert!(matches!(decode_cursor(&future), Err(TelemetryError::UnsupportedVersion(9))));
    }

    #[test]
    fn events_round_trip_through_jsonl() {
        let events = vec![
            InputEvent::MouseDown {
                t_us: 5,
                x: 1.0,
                y: 2.0,
                button: MouseButton::Left,
                clicks: 1,
                source: InputSource::Agent,
            },
            InputEvent::KeyDown {
                t_us: 9,
                label: Some("⌘S".into()),
                modifiers_only: false,
                redacted: false,
                source: InputSource::Person,
            },
        ];
        let mut buf = Vec::new();
        write_events(&mut buf, &events).unwrap();
        let back = read_events(&buf[..]).unwrap();
        assert_eq!(back, events);
        assert!(matches!(
            read_events(&b"{\"type\":\"nope\"}\n"[..]),
            Err(TelemetryError::Event { line: 1, .. })
        ));
    }

    #[test]
    fn interpolates_between_samples() {
        let s = vec![
            CursorSample { t_us: 0, x: 0.0, y: 0.0, ..Default::default() },
            CursorSample { t_us: 100, x: 10.0, y: 20.0, ..Default::default() },
        ];
        assert_eq!(cursor_at(&s, 50), Some(Point::new(5.0, 10.0)));
        assert_eq!(cursor_at(&s, 500), Some(Point::new(10.0, 20.0)));
        assert_eq!(cursor_at(&[], 5), None);
    }
}
