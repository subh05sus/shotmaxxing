import { PRODUCT_HUNT } from '@/lib/release';
import { cn } from '@/lib/utils';

// Product Hunt's own mark, drawn in its brand colours. design-guard-allow colour
const MARK = { circle: '#ff6154', letter: '#ffffff' }; // design-guard-allow colour

/** A link to the launch on Product Hunt, drawn in the site's own style. Nothing until PRODUCT_HUNT_URL is set. */
export function ProductHuntBadge({ className }: { className?: string }) {
  if (!PRODUCT_HUNT) return null;
  return (
    <a
      href={PRODUCT_HUNT}
      rel="noopener"
      className={cn(
        'press inline-flex h-11 items-center gap-2.5 rounded-full bg-graphite-2 pr-4 pl-1.5 text-[14px] font-semibold text-paper ring-1 ring-line-strong ring-inset hover:bg-graphite-3',
        className,
      )}
    >
      <svg viewBox="0 0 40 40" className="size-8" aria-hidden>
        <circle cx="20" cy="20" r="20" fill={MARK.circle} />
        <path d="M22.7 20H17v-6h5.7a3 3 0 0 1 0 6Zm0-10H13v20h4v-6h5.7a7 7 0 0 0 0-14Z" fill={MARK.letter} />
      </svg>
      Shotmaxxing on Product Hunt
    </a>
  );
}
