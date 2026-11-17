'use client';

/**
 * The hero's two videos, one per way of working, with their tabs on the player's top edge like a browser's, so they
 * read as part of the video rather than as a second button next to Download:
 *
 * - Record yourself: the real Shotmaxxing editor playing its sample project, filmed by scripts/record-hero.mjs.
 * - Let an AI agent do it: Claude Code in a terminal is asked to film a Notes demo, calls Shotmaxxing, and the
 *   video it made plays.
 *
 * A video plays only while it's on screen, so a page left open in the background costs nothing. With Reduce Motion
 * it waits for a click on Play instead of starting by itself.
 */
import { BotIcon, PauseCircleIcon, PlayCircleIcon, VideoCallIcon } from '@pikaicons/core-free';
import { Tabs } from 'radix-ui';
import { useEffect, useRef, useState } from 'react';
import { Icon } from '@/components/site/icon';
import { cn } from '@/lib/utils';

const VIDEOS = [
  {
    value: 'you',
    label: 'Record yourself',
    icon: VideoCallIcon,
    src: '/hero/hero',
    poster: '/hero/hero-poster.jpg',
    caption: 'The real editor, playing the sample project. Nothing here is mocked up.',
    alt: 'The Shotmaxxing editor playing a sign-up demo: the camera zooms in on each click and on typing, each zoom on the timeline says why it is there, and the tools change from Zoom to Cursor to Background.',
  },
  {
    value: 'agent',
    label: 'Let an AI agent do it',
    icon: BotIcon,
    src: '/hero/agent',
    poster: '/hero/agent-poster.jpg',
    caption:
      'Claude Code is asked for a Notes demo and calls Shotmaxxing, then the video it made plays. The wait is sped up.',
    alt: 'A terminal running Claude Code: someone types “Film a short demo of the Notes app with Shotmaxxing”, Claude plans the take and calls Shotmaxxing, then the finished video plays, with Notes writing a Weekend plans checklist.',
  },
] as const;

export function HeroVideo() {
  const [value, setValue] = useState<string>(VIDEOS[0].value);
  const [playing, setPlaying] = useState(false);
  const videos = useRef(new Map<string, HTMLVideoElement>());
  // Whether the person wants it playing: yes at first, unless Reduce Motion is on, and whatever they last chose.
  const wanted = useRef(true);
  useEffect(() => {
    wanted.current = !matchMedia('(prefers-reduced-motion: reduce)').matches;
  }, []);
  const shown = VIDEOS.find((v) => v.value === value) ?? VIDEOS[0];

  const toggle = () => {
    const v = videos.current.get(value);
    if (!v) return;
    wanted.current = v.paused;
    if (v.paused) void v.play().catch(() => {});
    else v.pause();
  };

  return (
    <Tabs.Root
      value={value}
      onValueChange={(v) => {
        setPlaying(false);
        setValue(v);
      }}
    >
      <figure>
        <div className="overflow-hidden rounded-[clamp(14px,2vw,26px)] bg-stage shadow-[0_60px_120px_-50px_var(--shadow),0_0_0_1px_var(--line-strong)]">
          <Tabs.List
            aria-label="Who films it"
            className="flex items-end gap-1 overflow-x-auto bg-graphite-2 px-2 pt-2 sm:px-3"
          >
            {VIDEOS.map((v) => (
              <Tabs.Trigger
                key={v.value}
                value={v.value}
                className={cn(
                  'group inline-flex h-10 shrink-0 items-center gap-2 rounded-t-[12px] px-3.5 text-[13.5px] font-medium whitespace-nowrap outline-none sm:h-11 sm:px-4 sm:text-[14.5px]',
                  'text-paper-muted transition-colors hover:text-paper focus-visible:ring-2 focus-visible:ring-lime focus-visible:ring-inset',
                  'data-[state=active]:bg-stage data-[state=active]:text-paper',
                )}
              >
                <Icon icon={v.icon} size={16} className="opacity-70 group-data-[state=active]:opacity-100" />
                {v.label}
                <span
                  aria-hidden
                  className="size-1.5 rounded-full bg-lime opacity-0 transition-opacity group-data-[state=active]:opacity-100"
                />
              </Tabs.Trigger>
            ))}
          </Tabs.List>
          {VIDEOS.map((v) => (
            <Tabs.Content key={v.value} value={v.value} className="outline-none">
              <Player
                {...v}
                wanted={wanted}
                onElement={(el) => (el ? videos.current.set(v.value, el) : videos.current.delete(v.value))}
                onPlaying={setPlaying}
              />
            </Tabs.Content>
          ))}
        </div>
        <figcaption className="mt-4 flex items-center justify-between gap-4 text-[13px] text-paper-subtle">
          <span>{shown.caption}</span>
          <button
            type="button"
            onClick={toggle}
            className="press hit relative inline-flex h-9 shrink-0 items-center gap-1.5 rounded-full bg-fill px-3.5 font-medium text-paper hover:bg-fill-hover"
            aria-label={playing ? 'Pause the video' : 'Play the video'}
          >
            <Icon icon={playing ? PauseCircleIcon : PlayCircleIcon} size={14} />
            {playing ? 'Pause' : 'Play'}
          </button>
        </figcaption>
      </figure>
    </Tabs.Root>
  );
}

/** One video: it plays while it's on screen and the person wants it to. */
function Player({
  src,
  poster,
  alt,
  wanted,
  onElement,
  onPlaying,
}: {
  src: string;
  poster: string;
  alt: string;
  wanted: React.RefObject<boolean>;
  onElement: (el: HTMLVideoElement | null) => void;
  onPlaying: (playing: boolean) => void;
}) {
  const video = useRef<HTMLVideoElement>(null);
  useEffect(() => {
    const v = video.current;
    if (!v) return;
    onElement(v);
    const io = new IntersectionObserver(
      ([e]) => {
        if (e.isIntersecting && wanted.current) void v.play().catch(() => {});
        else v.pause();
      },
      { threshold: 0.15 },
    );
    io.observe(v);
    return () => {
      io.disconnect();
      onElement(null);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- set up once per video
  }, []);

  // Both videos share one shape, so switching tabs doesn't move the page.
  return (
    <video
      ref={video}
      className="block aspect-[16/10] h-auto w-full object-cover"
      poster={poster}
      muted
      loop
      playsInline
      preload="metadata"
      onPlay={() => onPlaying(true)}
      onPause={() => onPlaying(false)}
      aria-label={alt}
    >
      <source src={`${src}.webm`} type="video/webm" />
      <source src={`${src}.mp4`} type="video/mp4" />
    </video>
  );
}
