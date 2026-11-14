/**
 * Settings that belong to this computer. Kept in local storage for now; the Rust core takes them over once the
 * control API lands, so the CLI and agents see the same values.
 */

import type { ThemeSetting } from '@shotmaxxing/ui';

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
  saveFolder: string;
  frameRate: 'adaptive60' | 'adaptive30' | 'adaptive120';
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
  saveFolder: '~/Movies/Shotmaxxing',
  frameRate: 'adaptive60',
  completedGuides: [],
};

const KEY = 'smx.settings';

function load(): Settings {
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? { ...DEFAULTS, ...JSON.parse(raw) } : { ...DEFAULTS };
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
