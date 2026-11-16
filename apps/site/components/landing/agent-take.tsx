'use client';

/**
 * An agent filming a demo, played once when it comes into view: the request types itself into a terminal, the take
 * card asks to allow it, ticks through the steps the agent planned, and the take finishes in the editor.
 */
import { CheckTickCircleIcon } from '@pikaicons/core-free';
import { useRef } from 'react';
import { gsap, useGSAP } from '@/lib/gsap';
import { Icon } from '@/components/site/icon';

const PROMPT = 'Film a 30-second demo of my app’s sign-up flow and export it for X.';
const STEPS = [
  'Open Halo at the sign-up page',
  'Type a sample email',
  'Click Sign up',
  'Show the welcome screen',
];

export function AgentTake() {
  const root = useRef<HTMLDivElement>(null);

  useGSAP(
    () => {
      const el = root.current;
      if (!el) return;
      const q = gsap.utils.selector(el);
      const typed = q('[data-prompt]')[0] as HTMLElement;
      if (matchMedia('(prefers-reduced-motion: reduce)').matches) {
        typed.textContent = PROMPT;
        gsap.set(q('[data-step-done]'), { autoAlpha: 1 });
        gsap.set(q('[data-card], [data-reply], [data-finished]'), { autoAlpha: 1, y: 0 });
        gsap.set(q('[data-ask]'), { autoAlpha: 0 });
        gsap.set(q('[data-controls]'), { autoAlpha: 1 });
        return;
      }
      gsap.set(q('[data-card]'), { autoAlpha: 0, y: 40, rotation: 2 });
      gsap.set(q('[data-reply], [data-finished]'), { autoAlpha: 0, y: 10 });
      gsap.set(q('[data-step-done]'), { autoAlpha: 0, scale: 0.4 });
      gsap.set(q('[data-step]'), { opacity: 0.45 });

      const tl = gsap.timeline({
        defaults: { ease: 'none' },
        scrollTrigger: { trigger: el, start: 'top 72%', once: true },
      });
      tl.to(
        { n: 0 },
        {
          n: PROMPT.length,
          duration: 2.1,
          onUpdate() {
            typed.textContent = PROMPT.slice(0, Math.round(this.targets()[0].n));
          },
        },
      )
        .to(q('[data-reply]'), { autoAlpha: 1, y: 0, duration: 0.6, ease: 'power2.out' })
        .to(
          q('[data-card]'),
          { autoAlpha: 1, y: 0, rotation: 0, duration: 1, ease: 'back.out(1.6)' },
          '-=0.2',
        )
        .to(q('[data-allow]'), { scale: 0.92, duration: 0.15, ease: 'power2.in' })
        .to(q('[data-allow]'), { scale: 1, duration: 0.4, ease: 'back.out(3)' })
        .to(q('[data-ask]'), { autoAlpha: 0, duration: 0.3 })
        .to(q('[data-controls]'), { autoAlpha: 1, duration: 0.3 }, '<');
      q('[data-step]').forEach((step, i) => {
        tl.to(step, { opacity: 1, duration: 0.3 }, `+=${i ? 0.35 : 0.2}`).to(
          q('[data-step-done]')[i],
          { autoAlpha: 1, scale: 1, duration: 0.45, ease: 'back.out(2.4)' },
          '<',
        );
      });
      tl.to(q('[data-finished]'), { autoAlpha: 1, y: 0, duration: 0.6, ease: 'power2.out' }, '+=0.2');
    },
    { scope: root },
  );

  return (
    <div ref={root} className="relative grid gap-5 md:grid-cols-[1.15fr_1fr] md:items-start">
      <div className="rounded-3xl border border-lime-ink/25 bg-stage p-5 font-mono text-[13.5px] leading-6 text-paper shadow-[0_40px_80px_-30px_var(--shadow)]">
        <div className="flex items-center gap-1.5 pb-4">
          <span className="size-2.5 rounded-full bg-graphite-4" />
          <span className="size-2.5 rounded-full bg-graphite-4" />
          <span className="size-2.5 rounded-full bg-graphite-4" />
          <span className="ml-3 text-[11px] tracking-wide text-paper-subtle">claude — ~/halo</span>
        </div>
        <p className="min-h-[3rem]">
          <span className="text-lime">› </span>
          <span data-prompt />
          <span className="ml-0.5 inline-block h-4 w-2 translate-y-0.5 bg-lime/80" />
        </p>
        <div data-reply className="mt-4 space-y-1.5 text-paper-muted">
          <p>
            <span className="text-lime">●</span> Reading the sign-up flow in the code…
          </p>
          <p>
            <span className="text-lime">●</span> Plan ready. Asking Shotmaxxing to film it.
          </p>
        </div>
        <p data-finished className="mt-4 text-paper">
          <span className="text-lime">✓</span> Done: <span className="text-lime">Halo sign-up.smx</span> is
          open in the editor, with 3 zooms. Exported <span className="text-paper-muted">halo-signup.mp4</span>{' '}
          (1:1, 1080p).
        </p>
      </div>

      <div
        data-card
        className="rounded-3xl bg-graphite-2 p-5 text-paper shadow-float ring-1 ring-line-strong"
      >
        <div className="flex items-center justify-between">
          <p className="text-[13px] font-semibold">Claude Code wants to film a take</p>
          <span className="flex items-center gap-1.5 rounded-full bg-record-tint px-2.5 py-1 font-mono text-[10.5px] font-semibold text-record">
            <span className="size-1.5 rounded-full bg-record" /> REC
          </span>
        </div>
        <p className="mt-1 text-[13px] text-paper-muted">Halo · the sign-up flow</p>
        <ol className="mt-4 space-y-2">
          {STEPS.map((s, i) => (
            <li
              key={s}
              data-step
              className="flex items-center gap-3 rounded-xl bg-fill px-3 py-2.5 text-[13.5px]"
            >
              <span className="relative grid size-6 shrink-0 place-items-center rounded-full bg-graphite-3 font-mono text-[11px] text-paper-muted">
                {i + 1}
                <span
                  data-step-done
                  className="absolute inset-0 grid place-items-center rounded-full bg-lime text-lime-ink"
                >
                  <Icon icon={CheckTickCircleIcon} size={16} />
                </span>
              </span>
              {s}
            </li>
          ))}
        </ol>
        <div className="relative mt-4 h-10">
          <div data-ask className="absolute inset-0 flex gap-2">
            <span className="grid flex-1 place-items-center rounded-full bg-fill text-[13px] font-medium text-paper-muted">
              Don’t allow
            </span>
            <span
              data-allow
              className="grid flex-1 place-items-center rounded-full bg-lime text-[13px] font-semibold text-lime-ink"
            >
              Allow
            </span>
          </div>
          <div data-controls className="absolute inset-0 flex gap-2 opacity-0">
            <span className="grid flex-1 place-items-center rounded-full bg-fill text-[13px] font-medium text-paper-muted">
              Pause
            </span>
            <span className="grid flex-1 place-items-center rounded-full bg-fill text-[13px] font-medium text-paper-muted">
              Take over
            </span>
            <span className="grid flex-1 place-items-center rounded-full bg-record-tint text-[13px] font-semibold text-record">
              Stop
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}
