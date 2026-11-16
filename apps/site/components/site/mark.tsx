import { cn } from '@/lib/utils';

/** The Shotmaxxing aperture: a ring round six blade edges (assets/brand/app-icon.svg). */
export function Mark({ className, strokeWidth = 1.9 }: { className?: string; strokeWidth?: number }) {
  return (
    <svg viewBox="-12 -12 24 24" className={cn('size-6', className)} aria-hidden="true">
      <g fill="none" stroke="currentColor" strokeWidth={strokeWidth} strokeLinecap="round">
        <circle r="10" />
        {[0, 60, 120, 180, 240, 300].map((a) => (
          <path key={a} d="M0 -3.6 L9.79 2.05" transform={`rotate(${a})`} />
        ))}
      </g>
    </svg>
  );
}

/** The app icon: the mark in ink on the lime tile. */
export function AppIcon({ className }: { className?: string }) {
  return (
    <span
      className={cn(
        'inline-grid size-8 place-items-center rounded-[28%] bg-gradient-to-b from-[var(--icon-tile-top)] to-[var(--icon-tile-bottom)] text-lime-ink shadow-[0_6px_16px_-6px_var(--shadow)]',
        className,
      )}
    >
      <Mark className="size-[66%]" />
    </span>
  );
}
