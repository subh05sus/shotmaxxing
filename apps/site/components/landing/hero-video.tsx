'use client';

/**
 * The hero's two videos, one per way of working, under capsule tabs:
 *
 * - Record yourself: the real Shotmaxxing editor playing its sample project, filmed by scripts/record-hero.mjs.
 * - Let an AI agent do it: Claude Code in a terminal is asked to film a Notes demo, calls Shotmaxxing, and the
 *   video it made plays.
 *
 * A video plays only while it's on screen, so a page left open in the background costs nothing. With Reduce Motion
 * it waits for a click on Play instead of starting by itself.
 */
import { PauseCircleIcon, PlayCircleIcon } from '@pikaicons/core-free';
import { useEffect, useRef, useState } from 'react';
import { CapsuleTabs, TabPanel } from '@/components/site/capsule-tabs';
import { Icon } from '@/components/site/icon';

const VIDEOS = [
  {
    value: 'you',
    label: 'Record yourself',
    src: '/hero/hero',
    poster: '/hero/hero-poster.jpg',
    caption: 'The real editor, playing the sample project. Nothing here is mocked up.',
    alt: 'The Shotmaxxing editor playing a sign-up demo: the camera zooms in on each click and on typing, each zoom on the timeline says why it is there, and the tools change from Zoom to Cursor to Background.',
  },
  {
    value: 'agent',
    label: 'Let an AI agent do it',
    src: '/hero/agent',
    poster: '/hero/agent-poster.jpg',
    caption:
      'Claude Code is asked for a Notes demo, calls Shotmaxxing, and this is the video it made. Real, uncut.',
    alt: 'A terminal running Claude Code: someone types “Film a short demo of the Notes app with Shotmaxxing”, Claude plans the take and calls Shotmaxxing, then the finished video plays, with Notes writing a Weekend plans checklist.',
  },
] as const;

export function HeroVideo() {
  return (
    <CapsuleTabs
      label="Who films it"
      defaultValue="you"
      className="flex flex-col items-center"
      items={VIDEOS.map(({ value, label }) => ({ value, label }))}
    >
      <div className="mt-5 w-full">
        {VIDEOS.map((v) => (
          <TabPanel key={v.value} value={v.value}>
            <Player {...v} />
          </TabPanel>
        ))}
      </div>
    </CapsuleTabs>
  );
}

function Player({
  src,
  poster,
  caption,
  alt,
}: {
  src: string;
  poster: string;
  caption: string;
  alt: string;
}) {
  const video = useRef<HTMLVideoElement>(null);
  const [playing, setPlaying] = useState(false);
  // Whether the person wants it playing: yes at first, unless Reduce Motion is on, and whatever they last chose.
  const wanted = useRef(true);

  useEffect(() => {
    const v = video.current;
    if (!v) return;
    wanted.current = !matchMedia('(prefers-reduced-motion: reduce)').matches;
    const io = new IntersectionObserver(
      ([e]) => {
        if (e.isIntersecting && wanted.current) void v.play().catch(() => {});
        else v.pause();
      },
      { threshold: 0.15 },
    );
    io.observe(v);
    return () => io.disconnect();
  }, []);

  const toggle = () => {
    const v = video.current;
    if (!v) return;
    wanted.current = v.paused;
    if (v.paused) void v.play().catch(() => {});
    else v.pause();
  };

  return (
    <figure className="group relative">
      <div className="overflow-hidden rounded-[clamp(14px,2vw,26px)] bg-stage shadow-[0_60px_120px_-50px_var(--shadow),0_0_0_1px_var(--line-strong)]">
        {/* Both videos share one shape, so switching tabs doesn't move the page. */}
        <video
          ref={video}
          className="block aspect-[16/10] h-auto w-full object-cover"
          poster={poster}
          muted
          loop
          playsInline
          preload="metadata"
          onPlay={() => setPlaying(true)}
          onPause={() => setPlaying(false)}
          aria-label={alt}
        >
          <source src={`${src}.webm`} type="video/webm" />
          <source src={`${src}.mp4`} type="video/mp4" />
        </video>
      </div>
      <figcaption className="mt-4 flex items-center justify-between gap-4 text-[13px] text-paper-subtle">
        <span>{caption}</span>
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
  );
}
