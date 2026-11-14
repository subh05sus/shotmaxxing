/**
 * Settings that belong to this computer. Kept in local storage for now; the Rust core takes them over once the
 * control API lands, so the CLI and agents see the same values.
 */

import type { ThemeSetting } from '@shotmaxxing/ui';
import type { RecordingOptions } from './native';

export type LowPowerSetting = 'automatic' | 'always_on' | 'always_off';

interface Settings {
  theme: ThemeSetting;
  lowPower: LowPowerSetting;
  onboardingDone: boolean;
  onboardingStep: number;
  tourDone: boolean;
  firstExportDone: boolean;
  countdown: boolean;
  showPill: boolean;
  hideDesktopIcons: boolean;
  launchAtLogin: boolean;
  menuBarIcon: boolean;
  /** Where recordings go, the short way (`~/…`). Empty means the default folder. */
  saveFolder: string;
  frameRate: 'adaptive60' | 'adaptive30' | 'adaptive120';
  checkUpdates: boolean;
  updatesCheckedAt: number;
  completedGuides: string[];
}

const DEFAULTS: Settings = {
  theme: 'system',
  lowPower: 'automatic',
  onboardingDone: false,
  onboardingStep: 0,
  tourDone: false,
  firstExportDone: false,
  countdown: true,
  showPill: true,
  hideDesktopIcons: false,
  launchAtLogin: false,
  menuBarIcon: true,
  saveFolder: '',
  frameRate: 'adaptive60',
  checkUpdates: true,
  updatesCheckedAt: 0,
  completedGuides: [],
};

// Earlier builds stored the default folder itself.
const OLD_DEFAULT_FOLDER = '~/Movies/Shotmaxxing';

const KEY = 'smx.settings';

function load(): Settings {
  try {
    const raw = localStorage.getItem(KEY);
    const loaded: Settings = raw ? { ...DEFAULTS, ...JSON.parse(raw) } : { ...DEFAULTS };
    if (loaded.saveFolder === OLD_DEFAULT_FOLDER) loaded.saveFolder = '';
    return loaded;
  } catch {
    return { ...DEFAULTS };
  }
}

export const settings = $state<Settings>(load());

/** Writes settings back. Called from an effect in App so every change is saved. */
export function persist(): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(settings));
    localStorage.setItem('smx.theme', settings.theme);
  } catch {
    /* private mode or full storage: settings still work for this session */
  }
}

export function resetSettings(): void {
  Object.assign(settings, DEFAULTS);
}

const FPS = { adaptive30: 30, adaptive60: 60, adaptive120: 120 } as const;

/** What a new recording should use, from Settings. */
export function recordingOptions(): RecordingOptions {
  return {
    fps: FPS[settings.frameRate] ?? 60,
    hideDesktopIcons: settings.hideDesktopIcons,
    folder: settings.saveFolder || undefined,
  };
}
