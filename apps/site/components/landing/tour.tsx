'use client';

/**
 * What Shotmaxxing does, one tab at a time: auto zoom, recording, the editor and the library. One picture of the real
 * app per tab, a few lines, and a link to the guide.
 */
import Image from 'next/image';
import Link from 'next/link';
import { ArrowRightIcon, FolderIcon, PencilEditIcon, TvIcon, SearchIcon } from '@pikaicons/core-free';
import { CapsuleTabs, TabPanel } from '@/components/site/capsule-tabs';
import { Icon } from '@/components/site/icon';
import { Keys } from '@/components/site/keycap';
import { SplitHeading } from '@/components/site/split-heading';

interface Stop {
  value: string;
  label: string;
  icon: typeof SearchIcon;
  title: string;
  points: React.ReactNode[];
  guide: [string, string];
  image: { src: string; width: number; height: number; alt: string };
}

const STOPS: Stop[] = [
  {
    value: 'zoom',
    label: 'Zoom',
    icon: SearchIcon,
    title: 'Zooms that know why they’re there.',
    points: [
      'Reads your clicks, your typing, the control you used and what changed on screen.',
      'Pushes in just before each moment and holds still through it.',
      <>
        Each zoom says why it’s there. Drag its edges, add one with <Keys keys="Z" />, or delete it.
      </>,
    ],
    guide: ['/docs/auto-zoom', 'How auto zoom decides'],
    image: {
      src: '/media/images/editor-zoom.png',
      width: 2720,
      height: 1720,
      alt: 'The Zoom tool: auto zoom on, how far and how smooth it zooms, and the four zooms in the video, each with the reason it is there.',
    },
  },
  {
    value: 'record',
    label: 'Record',
    icon: TvIcon,
    title: 'Press ⌥⇧5 and show your thing.',
    points: [
      'A screen, a window or an area, with the computer’s sound, a microphone and a camera.',
      'Window recordings keep their menus and sheets, and follow the window when it moves.',
      <>
        <Keys keys="⌥⇧4" /> takes a styled screenshot, framed and ready to copy.
      </>,
    ],
    guide: ['/docs/recording', 'The recording guide'],
    image: {
      src: '/media/images/record.png',
      width: 1680,
      height: 1200,
      alt: 'The record dialog: Video or Screenshot, Screen, Window or Area, the windows to choose from, and switches for the microphone, the camera and the computer’s sound.',
    },
  },
  {
    value: 'edit',
    label: 'Edit',
    icon: PencilEditIcon,
    title: 'A studio editor that already did the work.',
    points: [
      'A larger, smoother cursor with click effects, a spotlight and motion blur.',
      'Wallpapers, padding, corners and shadow, in 16:9, 4:3, 1:1 or 9:16.',
      'Captions, voice cleanup and silence cutting, all on your computer. Export MP4, GIF or PNG.',
    ],
    guide: ['/docs/editor', 'Every tool, explained'],
    image: {
      src: '/media/images/editor.png',
      width: 2720,
      height: 1720,
      alt: 'The Shotmaxxing editor with the sample project: the Background tool on the left, the recording in the middle on a green wallpaper, and the timeline with its zooms and effects below.',
    },
  },
  {
    value: 'library',
    label: 'Library',
    icon: FolderIcon,
    title: 'Every take in one place.',
    points: [
      'Recordings, screenshots and takes from agents, each marked with who filmed it.',
      'Every change saves itself, with version history to go back to.',
      <>
        <Keys keys="⌘K" /> finds any recording or action by name.
      </>,
    ],
    guide: ['/docs/setup', 'Get started'],
    image: {
      src: '/media/images/library.png',
      width: 2240,
      height: 1480,
      alt: 'The library: recordings and screenshots as cards, some marked as filmed by Claude Code or Codex, with Record and Screenshot at the bottom.',
    },
  },
];

export function Tour() {
  return (
    <section className="relative bg-graphite pt-24 pb-24 sm:pt-32 sm:pb-28" aria-labelledby="tour-title">
      <div className="shell">
        <div className="flex flex-col items-center text-center">
          <p className="eyebrow text-lime" data-rise>
            What it does
          </p>
          <SplitHeading className="display mt-4 max-w-[16ch] text-[clamp(2.4rem,5vw,4.2rem)]">
            <span id="tour-title">Record it once. It comes out edited.</span>
          </SplitHeading>
        </div>
        <div className="mt-10" data-rise>
          <CapsuleTabs
            label="What Shotmaxxing does"
            defaultValue="zoom"
            className="flex flex-col items-center"
            listClassName="max-sm:[&_svg]:hidden"
            items={STOPS.map(({ value, label, icon }) => ({ value, label, icon }))}
          >
            <div className="mt-8 w-full">
              {STOPS.map((s) => (
                <TabPanel key={s.value} value={s.value}>
                  <div className="grid items-center gap-8 lg:grid-cols-[0.75fr_1.25fr] lg:gap-12">
                    <div>
                      <h3 className="font-display text-[clamp(1.6rem,2.6vw,2.2rem)] leading-[1.05] font-extrabold tracking-[-0.03em]">
                        {s.title}
                      </h3>
                      <ul className="mt-5 grid gap-3">
                        {s.points.map((p, i) => (
                          <li key={i} className="flex gap-3 text-[15.5px] leading-relaxed text-paper-muted">
                            <span className="mt-[0.6em] size-1.5 shrink-0 rounded-full bg-lime" />
                            <span>{p}</span>
                          </li>
                        ))}
                      </ul>
                      <Link
                        href={s.guide[0]}
                        className="press group mt-7 inline-flex h-11 items-center gap-2 rounded-full bg-fill px-5 text-[14.5px] font-semibold text-paper ring-1 ring-line ring-inset hover:bg-fill-hover"
                      >
                        {s.guide[1]}
                        <Icon icon={ArrowRightIcon} size={16} act="nudge" />
                      </Link>
                    </div>
                    <div className="overflow-hidden rounded-[22px] bg-stage shadow-float ring-1 ring-line-strong">
                      <Image
                        src={s.image.src}
                        width={s.image.width}
                        height={s.image.height}
                        sizes="(min-width: 1216px) 720px, 100vw"
                        alt={s.image.alt}
                        className="h-auto w-full"
                      />
                    </div>
                  </div>
                </TabPanel>
              ))}
            </div>
          </CapsuleTabs>
        </div>
      </div>
    </section>
  );
}
