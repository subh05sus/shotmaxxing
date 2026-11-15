/**
 * Two small sounds, made on the spot rather than shipped as files: the soft click of the logo's blades locking on
 * the first launch, and the shutter that closes setup. Each is a few bursts of filtered noise (the metal) over a
 * short low thump (the body), quiet and under a tenth of a second. No audio device, or a blocked one, means silence.
 */

let context: AudioContext | undefined;

function audio(): AudioContext | undefined {
  try {
    context ??= new AudioContext();
    if (context.state === 'suspended') void context.resume();
    return context;
  } catch {
    return undefined;
  }
}

function burst(
  ctx: BaseAudioContext,
  out: AudioNode,
  at: number,
  freq: number,
  q: number,
  decay: number,
  level: number,
) {
  const length = Math.ceil(ctx.sampleRate * (decay + 0.02));
  const buffer = ctx.createBuffer(1, length, ctx.sampleRate);
  const data = buffer.getChannelData(0);
  for (let i = 0; i < length; i++) data[i] = Math.random() * 2 - 1;
  const source = ctx.createBufferSource();
  source.buffer = buffer;
  const filter = ctx.createBiquadFilter();
  filter.type = 'bandpass';
  filter.frequency.value = freq;
  filter.Q.value = q;
  const gain = ctx.createGain();
  gain.gain.setValueAtTime(0, at);
  gain.gain.linearRampToValueAtTime(level, at + 0.0015);
  gain.gain.exponentialRampToValueAtTime(0.0005, at + decay);
  source.connect(filter).connect(gain).connect(out);
  source.start(at);
  source.stop(at + decay + 0.02);
}

function thump(
  ctx: BaseAudioContext,
  out: AudioNode,
  at: number,
  freq: number,
  decay: number,
  level: number,
) {
  const osc = ctx.createOscillator();
  osc.type = 'sine';
  osc.frequency.setValueAtTime(freq, at);
  osc.frequency.exponentialRampToValueAtTime(freq * 0.55, at + decay);
  const gain = ctx.createGain();
  gain.gain.setValueAtTime(0, at);
  gain.gain.linearRampToValueAtTime(level, at + 0.003);
  gain.gain.exponentialRampToValueAtTime(0.0005, at + decay);
  osc.connect(gain).connect(out);
  osc.start(at);
  osc.stop(at + decay + 0.02);
}

/**
 * `lock`: one soft tick. `shutter`: the two-part ka-chick of a shutter closing and catching. `into` renders it
 * somewhere else instead of the speakers (an OfflineAudioContext, to check how loud it is).
 */
export function playClick(kind: 'lock' | 'shutter', into?: BaseAudioContext): void {
  const ctx = into ?? audio();
  if (!ctx) return;
  try {
    const at = ctx.currentTime + 0.005;
    const out = ctx.createGain();
    out.gain.value = kind === 'lock' ? 0.14 : 0.18;
    out.connect(ctx.destination);
    if (kind === 'lock') {
      burst(ctx, out, at, 3400, 1.3, 0.03, 0.9);
      burst(ctx, out, at + 0.008, 7000, 2.2, 0.012, 0.35);
      thump(ctx, out, at, 190, 0.05, 0.45);
    } else {
      burst(ctx, out, at, 2300, 0.9, 0.045, 0.85);
      thump(ctx, out, at, 150, 0.07, 0.5);
      burst(ctx, out, at + 0.062, 3900, 1.4, 0.035, 0.75);
      thump(ctx, out, at + 0.062, 230, 0.04, 0.32);
    }
  } catch {
    // A sound that can't play is just silence.
  }
}
