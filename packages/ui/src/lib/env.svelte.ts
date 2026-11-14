/**
 * The environment every component reads: which OS look to use, the resolved theme, and how much motion is allowed.
 * The app sets these once at start-up (and again when the system or the power policy changes).
 */

export type OS = 'mac' | 'windows';
export type Theme = 'light' | 'dark';
export type ThemeSetting = 'system' | Theme;
export type MotionLevel = 'full' | 'fades';

export const env = $state({
  os: 'mac' as OS,
  theme: 'dark' as Theme,
  themeSetting: 'system' as ThemeSetting,
  /** Native translucent material (vibrancy / Mica) is drawn behind the chrome. */
  material: false,
  /** Reduce Motion from the OS. */
  reducedMotion: false,
  /** Low power from the power policy. */
  lowPower: false,
});

export function motionLevel(): MotionLevel {
  return env.reducedMotion || env.lowPower ? 'fades' : 'full';
}

/** Mirrors the environment onto <html> so CSS can follow it. */
export function applyEnv(root: HTMLElement = document.documentElement): void {
  root.dataset.os = env.os;
  root.dataset.theme = env.theme;
  root.dataset.motion = motionLevel();
  if (env.material) root.dataset.material = 'native';
  else delete root.dataset.material;
}

/** Follows the system theme and Reduce Motion. Returns a function that stops following. */
export function watchSystem(onChange: () => void = () => applyEnv()): () => void {
  const dark = matchMedia('(prefers-color-scheme: dark)');
  const reduce = matchMedia('(prefers-reduced-motion: reduce)');
  const update = () => {
    env.theme = env.themeSetting === 'system' ? (dark.matches ? 'dark' : 'light') : env.themeSetting;
    env.reducedMotion = reduce.matches;
    onChange();
  };
  update();
  dark.addEventListener('change', update);
  reduce.addEventListener('change', update);
  return () => {
    dark.removeEventListener('change', update);
    reduce.removeEventListener('change', update);
  };
}

export function setThemeSetting(setting: ThemeSetting): void {
  env.themeSetting = setting;
  env.theme =
    setting === 'system' ? (matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light') : setting;
  applyEnv();
}
