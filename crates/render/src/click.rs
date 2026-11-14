//! The click sound under each press. It is made from a few short tones rather than a recording, so the editor and
//! the export make the same sound and no sound file has to ship with the app.

use std::f64::consts::TAU;

/// How long a click lasts, in seconds.
pub const CLICK_SECONDS: f64 = 0.045;
/// The loudest sample, about -9 dB: clear over a voice without startling anyone.
const PEAK: f64 = 0.35;
/// Frequency in Hz, decay time in seconds and level: a bright tick over a soft, low knock.
const TONES: [(f64, f64, f64); 4] =
    [(1850.0, 0.006, 0.55), (3100.0, 0.0035, 0.3), (5300.0, 0.0018, 0.15), (180.0, 0.012, 0.25)];

/// One click at `rate` samples a second. It rises over 0.4 ms and fades out over its last 5 ms, so neither edge pops.
pub fn click_samples(rate: u32) -> Vec<f32> {
    let rate = f64::from(rate.max(1));
    let n = (CLICK_SECONDS * rate).round() as usize;
    let raw: Vec<f64> = (0..n)
        .map(|i| {
            let t = i as f64 / rate;
            let envelope = (t / 0.0004).min(1.0) * ((CLICK_SECONDS - t) / 0.005).clamp(0.0, 1.0);
            let tones: f64 =
                TONES.iter().map(|(f, decay, level)| level * (TAU * f * t).sin() * (-t / decay).exp()).sum();
            envelope * tones
        })
        .collect();
    let peak = raw.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    let scale = if peak > 0.0 { PEAK / peak } else { 0.0 };
    raw.into_iter().map(|x| (x * scale) as f32).collect()
}

/// `samples` as a mono, 16-bit WAV file.
pub fn wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // The format chunk's size.
    out.extend_from_slice(&1u16.to_le_bytes()); // Integer PCM.
    out.extend_from_slice(&1u16.to_le_bytes()); // One channel.
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes()); // Bytes a second.
    out.extend_from_slice(&2u16.to_le_bytes()); // Bytes a frame.
    out.extend_from_slice(&16u16.to_le_bytes()); // Bits a sample.
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_is_short_soft_edged_and_at_its_level() {
        let click = click_samples(48_000);
        assert_eq!(click.len(), 2160);
        let peak = click.iter().fold(0f32, |m, x| m.max(x.abs()));
        assert!((f64::from(peak) - PEAK).abs() < 1e-4, "{peak}");
        assert_eq!(click[0], 0.0);
        assert!(click[click.len() - 1].abs() < 1e-3, "{}", click[click.len() - 1]);
        // Most of the energy is in the first 15 ms: a tick, not a tone.
        let energy = |s: &[f32]| s.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>();
        assert!(energy(&click[..720]) > 0.9 * energy(&click));
    }

    #[test]
    fn writes_a_wav_header_that_matches_the_samples() {
        let file = wav(&[0.0, 1.0, -1.0], 48_000);
        assert_eq!(&file[..4], b"RIFF");
        assert_eq!(&file[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(file[4..8].try_into().unwrap()), 36 + 6);
        assert_eq!(&file[36..40], b"data");
        assert_eq!(u32::from_le_bytes(file[40..44].try_into().unwrap()), 6);
        assert_eq!(file.len(), 50);
        assert_eq!(i16::from_le_bytes([file[46], file[47]]), 32767);
        assert_eq!(i16::from_le_bytes([file[48], file[49]]), -32767);
    }
}
