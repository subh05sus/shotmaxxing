'use client';

/**
 * The real Shotmaxxing editor playing its sample project, filmed by scripts/record-hero.mjs, on a loop.
 *
 * It plays only while it's on screen, so a page left open in the background costs nothing. With Reduce Motion it
 * waits for a click on Play instead of starting by itself.
 */
import { PauseCircleIcon, PlayCircleIcon } from '@pikaicons/core-free';
import { useEffect, useRef, useState } from 'react';
import { Icon } from '@/components/site/icon';

export function HeroVideo() {
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
        <video
          ref={video}
          className="block aspect-[16/10] h-auto w-full"
          poster="/hero/hero-poster.jpg"
          muted
          loop
          playsInline
          preload="auto"
          onPlay={() => setPlaying(true)}
          onPause={() => setPlaying(false)}
          aria-label="The Shotmaxxing editor playing a sign-up demo: the camera zooms in on each click and on typing, each zoom on the timeline says why it is there, and the tools change from Zoom to Cursor to Background."
        >
          <source src="/hero/hero.webm" type="video/webm" />
          <source src="/hero/hero.mp4" type="video/mp4" />
        </video>
      </div>
      <figcaption className="mt-4 flex items-center justify-between gap-4 text-[13px] text-paper-subtle">
        <span>The real editor, playing the sample project. Nothing here is mocked up.</span>
        <button
          type="button"
          onClick={toggle}
          className="press hit relative inline-flex h-9 shrink-0 items-center gap-1.5 rounded-full bg-fill px-3.5 font-medium text-paper hover:bg-fill-hover"
          aria-label={playing ? 'Pause the video' : 'Play the video'}
        >
          <Icon icon={playing ? PauseCircleIcon : PlayCircleIcon} size={14} act="pop" />
          {playing ? 'Pause' : 'Play'}
        </button>
      </figcaption>
    </figure>
  );
}
