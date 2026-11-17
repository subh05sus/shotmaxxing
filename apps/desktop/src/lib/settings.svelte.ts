/**
 * Settings that belong to this computer. Kept in local storage for now; the Rust core takes them over once the
 * control API lands, so the CLI and agents see the same values.
 */

import type { ThemeSetting } from '@shotmaxxing/ui';
import type { RecordingOptions, StillLook } from './native';

export type LowPowerSetting = 'automatic' | 'always_on' | 'always_off';

interface Settings {
  theme: ThemeSetting;
  lowPower: LowPowerSetting;
  onboardingDone: boolean;
  onboardingStep: number;
  tourDone: boolean;
  /** The shorter tour a first screenshot gets. */
  stillTourDone: boolean;
  /** The look of the last screenshot someone styled, which the next one starts from. */
  stillLook: StillLook | null;
  firstExportDone: boolean;
  countdown: boolean;
  showPill: boolean;
  hideDesktopIcons: boolean;
  launchAtLogin: boolean;
  menuBarIcon: boolean;
  /** Where recordings go, the short way (`~/…`). Empty means the default folder. */
  saveFolder: string;
  /** Where exports go, the short way. Empty means next to the project. */
  exportFolder: string;
  frameRate: 'adaptive60' | 'adaptive30' | 'adaptive120';
  /** Record what the computer plays. */
  systemAudio: boolean;
  microphone: boolean;
  /** The chosen microphone; empty means the system's default. */
  microphoneId: string;
  camera: boolean;
  /** The chosen camera; empty means the system's default. */
  cameraId: string;
  checkUpdates: boolean;
  updatesCheckedAt: number;
  completedGuides: string[];
  /** The person has read what Allow everything means, once; turning it on again doesn't ask. */
  allowEverythingConfirmed: boolean;
  /** Screen Recording has worked at a launch. Should it stop (an update can make macOS forget it), the app says so,
   *  once. `null` when settings come from a build before 0.2.1, which didn't note it: the next launch finds out. */
  screenAllowedBefore: boolean | null;
  /** Background colours and gradients made in the editor, newest first, offered again under Recent. */
  recentColors: string[];
  recentGradients: { from: string; to: string; angle: number }[];
  /** A version the person chose to skip: it isn't offered again until a newer one comes out. */
  skippedVersion: string;
  /** The version that last opened, so the first launch after an update can show what's new. */
  lastOpenedVersion: string;
}

const DEFAULTS: Settings = {
  theme: 'system',
  lowPower: 'automatic',
  onboardingDone: false,
  onboardingStep: 0,
  tourDone: false,
  stillTourDone: false,
  stillLook: null,
  firstExportDone: false,
  countdown: true,
  showPill: true,
  hideDesktopIcons: false,
  launchAtLogin: false,
  menuBarIcon: true,
  saveFolder: '',
  exportFolder: '',
  frameRate: 'adaptive60',
  systemAudio: true,
  microphone: true,
  microphoneId: '',
  camera: false,
  cameraId: '',
  checkUpdates: true,
  updatesCheckedAt: 0,
  completedGuides: [],
  allowEverythingConfirmed: false,
  screenAllowedBefore: false,
  recentColors: [],
  recentGradients: [],
  skippedVersion: '',
  lastOpenedVersion: '',
};

// Earlier builds stored the default folder itself.
const OLD_DEFAULT_FOLDER = '~/Movies/Shotmaxxing';

const KEY = 'smx.settings';

function load(): Settings {
  try {
    const raw = localStorage.getItem(KEY);
    const stored = raw ? (JSON.parse(raw) as Partial<Settings>) : null;
    const loaded: Settings = { ...DEFAULTS, ...stored };
    if (loaded.saveFolder === OLD_DEFAULT_FOLDER) loaded.saveFolder = '';
    // Builds before 0.2.1 didn't note whether Screen Recording worked, and their setup could be skipped without it.
    if (stored && !('screenAllowedBefore' in stored) && loaded.onboardingDone)
      loaded.screenAllowedBefore = null;
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
    systemAudio: settings.systemAudio,
    microphone: settings.microphone ? settings.microphoneId || 'default' : undefined,
    camera: settings.camera ? settings.cameraId || 'default' : undefined,
  };
}
