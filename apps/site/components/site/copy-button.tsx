'use client';

import { CopyIcon } from '@pikaicons/core-free';
import { useState } from 'react';
import { cn } from '@/lib/utils';
import { CheckDraw } from './check';
import { Icon } from './icon';

/**
 * Copies `text`. The answer is instant (the check draws while the clipboard write finishes) and quiet: people copy
 * often, so it doesn't bounce.
 */
export function CopyButton({
  text,
  className,
  label = 'Copy',
  tone = 'dark',
}: {
  text: string;
  className?: string;
  label?: string;
  tone?: 'dark' | 'lime';
}) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      onClick={() => {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1600);
        navigator.clipboard?.writeText(text).catch(() => setCopied(false));
      }}
      className={cn(
        'press hit inline-flex h-8 shrink-0 items-center gap-1.5 rounded-full px-3 text-xs font-medium',
        tone === 'dark'
          ? 'bg-fill text-paper-muted hover:bg-fill-hover hover:text-paper'
          : 'bg-lime-ink text-lime hover:bg-stage',
        copied && (tone === 'dark' ? 'text-lime hover:text-lime' : ''),
        className,
      )}
      aria-label={copied ? 'Copied' : `${label} to the clipboard`}
    >
      {copied ? <CheckDraw size={14} /> : <Icon icon={CopyIcon} size={14} />}
      <span aria-live="polite">{copied ? 'Copied' : label}</span>
    </button>
  );
}
