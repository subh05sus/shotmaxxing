import { cn } from '@/lib/utils';

/** A key, drawn as a keycap. `keys` like "⌥⇧5" show as one cap per symbol. */
export function Keys({
  keys,
  className,
  tone = 'dark',
}: {
  keys: string;
  className?: string;
  tone?: 'dark' | 'lime';
}) {
  return (
    <span className={cn('inline-flex translate-y-[-1px] items-center gap-1 align-middle', className)}>
      {[...keys].map((k, i) => (
        <kbd
          key={i}
          className={cn(
            'inline-grid h-6 min-w-6 place-items-center rounded-md px-1.5 font-sans text-[12px] leading-none font-semibold',
            tone === 'dark'
              ? 'bg-graphite-3 text-paper shadow-[inset_0_-2px_0_var(--line-strong),0_1px_0_var(--line)]'
              : 'bg-lime-ink text-lime shadow-[inset_0_-2px_0_var(--black)]',
          )}
        >
          {k}
        </kbd>
      ))}
    </span>
  );
}
