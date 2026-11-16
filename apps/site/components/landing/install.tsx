'use client';

import { AppleLogoIcon, DownloadDownIcon, Package01Icon } from '@pikaicons/core-free';
import { WindowsLogoIcon } from '@/components/site/logos';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import type { Release } from '@/lib/release';
import { CapsuleTabs, TabPanel } from '@/components/site/capsule-tabs';
import { CopyButton } from '@/components/site/copy-button';
import { Icon } from '@/components/site/icon';
import { SplitHeading } from '@/components/site/split-heading';
import { useOs } from '@/lib/os';

function Command({ lines }: { lines: string[] }) {
  return (
    <div className="flex items-start gap-3 rounded-2xl border border-line bg-code p-2 pl-4">
      <pre className="scroll-room min-w-0 flex-1 overflow-x-auto py-1.5 font-mono text-[13.5px] leading-7 text-paper sm:text-[14.5px]">
        {lines.map((l) => (
          <div key={l}>
            <span className="text-lime select-none">$ </span>
            {l}
          </div>
        ))}
      </pre>
      <CopyButton text={lines.join('\n')} />
    </div>
  );
}

function Notes({ children }: { children: React.ReactNode }) {
  return (
    <ul className="mt-5 grid gap-3 text-[15px] leading-relaxed text-paper-muted sm:grid-cols-3">
      {children}
    </ul>
  );
}

function Note({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <li className="rounded-2xl bg-fill p-4">
      <p className="font-semibold text-paper">{title}</p>
      <p className="mt-1 text-[14px]">{children}</p>
    </li>
  );
}

export function Install({ release }: { release: Release }) {
  const [os, setOs] = useState('mac');
  // The visitor's computer, found by itself, or the one a logo or button picked.
  const picked = useOs();
  useEffect(() => {
    if (picked) setOs(picked);
  }, [picked]);
  const f = release.files;

  return (
    <section id="install" className="relative bg-graphite py-24 sm:py-32" aria-labelledby="install-title">
      <div className="shell">
        <p className="eyebrow text-lime" data-rise>
          Install
        </p>
        <SplitHeading className="display mt-4 max-w-[14ch] text-[clamp(2.6rem,6vw,5rem)]">
          <span id="install-title">One line, and it opens on the first click.</span>
        </SplitHeading>
        <p className="measure mt-5 text-[17px] leading-relaxed text-paper-muted" data-rise>
          Each line downloads version {release.version} from GitHub, checks it against its SHA-256, installs
          it and opens it, with no “Not Opened” dialog on the Mac and no SmartScreen warning on Windows. Run
          it again to update.
        </p>

        <div className="mt-10" data-rise>
          <CapsuleTabs
            label="Your computer"
            listClassName="max-sm:[&_svg]:hidden"
            value={os}
            onValueChange={setOs}
            items={[
              { value: 'mac', label: 'macOS', icon: AppleLogoIcon },
              { value: 'windows', label: 'Windows', icon: WindowsLogoIcon },
              { value: 'brew', label: 'Homebrew', icon: Package01Icon },
              { value: 'files', label: 'Download', icon: DownloadDownIcon },
            ]}
          >
            <div className="mt-6">
              <TabPanel value="mac">
                <p className="mb-3 text-[15px] text-paper-muted">
                  <span className="font-semibold text-paper">macOS 14 or later</span>, Apple silicon or Intel.
                  Paste this into Terminal:
                </p>
                <Command lines={[release.commands.mac]} />
                <Notes>
                  <Note title="Goes in Applications">
                    And the <code className="font-mono text-paper">shotmaxxing</code> command is linked onto
                    your PATH when a folder there is yours to change.
                  </Note>
                  <Note title="Short and readable">
                    The whole script is on GitHub, so you can read install.sh before you run it.
                  </Note>
                  <Note title="Then turn on permissions">
                    Setup asks for Screen Recording and shows each switch live.{' '}
                    <Link href="/docs/permissions" className="text-lime underline-offset-4 hover:underline">
                      Permissions guide
                    </Link>
                  </Note>
                </Notes>
              </TabPanel>
              <TabPanel value="windows">
                <p className="mb-3 text-[15px] text-paper-muted">
                  <span className="font-semibold text-paper">Windows 10 (version 2004) or later</span>,
                  64-bit. Paste this into PowerShell:
                </p>
                <Command lines={[release.commands.windows]} />
                <Notes>
                  <Note title="Just for you">Installs for your user, without administrator rights.</Note>
                  <Note title="A preview">
                    Recording, export and agent takes on Windows are written but not yet tested on a Windows
                    computer, so expect rough edges.
                  </Note>
                  <Note title="Update the same way">Run the line again whenever a new version is out.</Note>
                </Notes>
              </TabPanel>
              <TabPanel value="brew">
                <p className="mb-3 text-[15px] text-paper-muted">
                  With <span className="font-semibold text-paper">Homebrew</span> on a Mac. It opens on the
                  first click this way too, and <code className="font-mono text-paper">brew upgrade</code>{' '}
                  updates it:
                </p>
                <Command lines={release.commands.brew} />
                <Notes>
                  <Note title="This repository is the tap">No other tap or account needed.</Note>
                  <Note title="The command comes too">
                    <code className="font-mono text-paper">shotmaxxing</code> goes on your PATH with the app.
                  </Note>
                  <Note title="Remove it">
                    <code className="font-mono text-paper">brew uninstall --cask shotmaxxing</code>
                  </Note>
                </Notes>
              </TabPanel>
              <TabPanel value="files">
                <ul className="grid gap-3 sm:grid-cols-2">
                  {[
                    { label: 'Mac with Apple silicon', file: f.macArm },
                    { label: 'Mac with Intel', file: f.macIntel },
                    { label: 'Windows, setup program', file: f.winSetup },
                    { label: 'Windows, MSI', file: f.winMsi },
                  ].map(({ label, file }) => (
                    <li key={file.name}>
                      <a
                        href={file.url}
                        className="press group flex items-center gap-4 rounded-2xl border border-line bg-graphite-2 p-4 hover:border-line-strong hover:bg-graphite-3"
                      >
                        <span className="grid size-11 place-items-center rounded-xl bg-lime text-lime-ink">
                          <Icon icon={DownloadDownIcon} size={19} act="drop" />
                        </span>
                        <span className="min-w-0">
                          <span className="block font-semibold">{label}</span>
                          <span className="block truncate font-mono text-[12px] text-paper-subtle">
                            {file.name}
                          </span>
                        </span>
                      </a>
                    </li>
                  ))}
                </ul>
                <p className="measure mt-5 text-[14px] leading-relaxed text-paper-muted">
                  The builds aren&apos;t notarized by Apple or signed for Windows yet, so a file downloaded in
                  a browser gets one warning the first time it opens.{' '}
                  <Link
                    href="/docs/install#download-the-installer-yourself"
                    className="text-lime hover:underline"
                  >
                    How to open it
                  </Link>
                  . Every version is on the{' '}
                  <a href={release.all} className="text-lime hover:underline">
                    Releases page
                  </a>
                  .
                </p>
              </TabPanel>
            </div>
          </CapsuleTabs>
        </div>
      </div>
    </section>
  );
}
