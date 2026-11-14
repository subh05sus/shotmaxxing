/**
 * The click sound under each press, made from a few short tones exactly as the export makes it
 * (`crates/render/src/click.rs`), so the preview sounds like the finished video.
 */
export const CLICK_SECONDS = 0.045;
/** The loudest sample, about -9 dB. */
const PEAK = 0.35;
/** Frequency in Hz, decay time in seconds and level: a bright tick over a soft, low knock. */
const TONES: [number, number, number][] = [
  [1850, 0.006, 0.55],
  [3100, 0.0035, 0.3],
  [5300, 0.0018, 0.15],
  [180, 0.012, 0.25],
];

/** One click at `rate` samples a second. It rises over 0.4 ms and fades out over its last 5 ms. */
export function clickSamples(rate: number): Float32Array<ArrayBuffer> {
  const n = Math.round(CLICK_SECONDS * rate);
  const out = new Float32Array(n);
  let peak = 0;
  for (let i = 0; i < n; i++) {
    const t = i / rate;
    const envelope = Math.min(1, t / 0.0004) * Math.min(1, Math.max(0, (CLICK_SECONDS - t) / 0.005));
    let sum = 0;
    for (const [f, decay, level] of TONES)
      sum += level * Math.sin(2 * Math.PI * f * t) * Math.exp(-t / decay);
    out[i] = envelope * sum;
    peak = Math.max(peak, Math.abs(out[i]));
  }
  if (peak > 0) for (let i = 0; i < n; i++) out[i] *= PEAK / peak;
  return out;
}
