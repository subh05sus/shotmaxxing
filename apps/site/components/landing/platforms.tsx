'use client';

/**
 * The hero's install button and the platform logos under it. Both follow the visitor's computer: the button says
 * which download it's for, and the logo for their computer is filled in. A logo opens the Install section on that
 * computer's tab.
 */
import { DownloadDownIcon } from '@pikaicons/core-free';
import { useState } from 'react';
import { CopyButton } from '@/components/site/copy-button';
import { Icon } from '@/components/site/icon';
import { AppleLogo, WindowsLogo } from '@/components/site/logos';
import { chooseOs, useOs, type Os } from '@/lib/os';
import { cn } from '@/lib/utils';

const PLATFORMS: { os: Os; label: string; hint: string; Logo: typeof AppleLogo }[] = [
  { os: 'mac', label: 'Install on macOS', hint: 'macOS 14 or later', Logo: AppleLogo },
  { os: 'windows', label: 'Install on Windows', hint: 'Windows 10 or later, in preview', Logo: WindowsLogo },
];

export function HeroInstall() {
  const os = useOs();
  return (
    <a
      href="#install"
      onClick={(e) => {
        if (!os) return;
        e.preventDefault();
        chooseOs(os);
      }}
      className="press group inline-flex h-13 items-center gap-2.5 rounded-full bg-lime-ink px-7 text-[16px] font-semibold text-lime shadow-[0_14px_30px_-12px_var(--shadow)] hover:bg-stage"
    >
      <Icon icon={DownloadDownIcon} size={19} act="drop" />
      {os === 'mac' ? 'Download for Mac' : os === 'windows' ? 'Download for Windows' : 'Install free'}
    </a>
  );
}

export function PlatformLinks() {
  const os = useOs();
  return (
    <ul className="flex items-center justify-center gap-2.5" aria-label="Free and open source, for">
      {PLATFORMS.map(({ os: o, label, hint, Logo }) => (
        <li key={o}>
          <a
            href="#install"
            onClick={(e) => {
              e.preventDefault();
              chooseOs(o);
            }}
            aria-label={`${label} (${hint})`}
            title={hint}
            className={cn(
              'press hit grid size-11 place-items-center rounded-full transition-colors',
              os === o
                ? 'bg-lime-ink text-lime'
                : 'bg-lime-ink-subtle text-lime-ink hover:bg-lime-ink hover:text-lime',
            )}
          >
            <Logo size={19} />
          </a>
        </li>
      ))}
    </ul>
  );
}

/** The final call to action's one line, for the visitor's computer, with the other one a click away. */
export function InstallLine({ mac, windows }: { mac: string; windows: string }) {
  const detected = useOs();
  const [picked, setPicked] = useState<Os | null>(null);
  const os = picked ?? detected ?? 'mac';
  const line = os === 'windows' ? windows : mac;
  const other: Os = os === 'windows' ? 'mac' : 'windows';
  return (
    <>
      <div
        className="mx-auto mt-10 flex max-w-2xl items-center gap-3 rounded-full bg-lime-ink p-2 pl-6 text-left"
        data-rise
      >
        <code className="scroll-room min-w-0 flex-1 overflow-x-auto font-mono text-[13px] whitespace-nowrap text-lime sm:text-[14px]">
          {line}
        </code>
        <CopyButton text={line} tone="lime" className="h-10 bg-lime px-4 text-lime-ink hover:bg-lime-hover" />
      </div>
      <p className="mt-5 text-[14px] text-lime-ink-muted" data-rise>
        Paste it into {os === 'windows' ? 'PowerShell' : 'Terminal'}.{' '}
        <button
          type="button"
          onClick={() => setPicked(other)}
          className="hit relative inline-flex items-center gap-1.5 font-semibold text-lime-ink underline decoration-lime-ink/40 underline-offset-3 hover:decoration-lime-ink"
        >
          {other === 'windows' ? <WindowsLogo size={13} /> : <AppleLogo size={13} />}
          {other === 'windows' ? 'On Windows?' : 'On a Mac?'}
        </button>
      </p>
    </>
  );
}
