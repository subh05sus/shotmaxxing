<script lang="ts">
  import type { Snippet } from 'svelte';
  import { X } from '../lib/icons';
  import { fade, pop, unpop } from '../lib/motion';
  import { portal } from '../lib/portal';
  import IconButton from './IconButton.svelte';

  interface Props {
    open: boolean;
    title: string;
    description?: string;
    width?: number;
    /** Where the sheet grows from, e.g. the button that opened it. */
    origin?: string;
    onclose?: () => void;
    children: Snippet;
    footer?: Snippet;
  }

  let {
    open = $bindable(false),
    title,
    description,
    width = 520,
    origin = 'center',
    onclose,
    children,
    footer,
  }: Props = $props();

  const id = $props.id();
  let panel: HTMLDivElement | undefined = $state();
  let returnFocus: Element | null = null;

  function close() {
    open = false;
    onclose?.();
  }

  $effect(() => {
    if (!open) return;
    returnFocus = document.activeElement;
    queueMicrotask(() => {
      const first = panel?.querySelector<HTMLElement>(
        '[autofocus], input, button:not([aria-label="Close"]), [tabindex="0"]',
      );
      (first ?? panel)?.focus();
    });
    return () => (returnFocus as HTMLElement | null)?.focus?.();
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      close();
      return;
    }
    if (e.key !== 'Tab' || !panel) return;
    // Keep focus inside the sheet.
    const focusable = [
      ...panel.querySelectorAll<HTMLElement>('button, input, select, textarea, [tabindex="0"]'),
    ].filter((el) => !el.hasAttribute('disabled'));
    if (!focusable.length) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (e.shiftKey && document.activeElement === first) (e.preventDefault(), last.focus());
    else if (!e.shiftKey && document.activeElement === last) (e.preventDefault(), first.focus());
  }
</script>

{#if open}
  <div use:portal class="fixed inset-0 z-[800] grid place-items-center p-6">
    <!-- Staging: everything behind the sheet dims, so the eye goes to one place. -->
    <div
      class="absolute inset-0 bg-scrim"
      in:fade={{ duration: 180 }}
      out:fade={{ duration: 120 }}
      onclick={close}
      role="presentation"
    ></div>
    <div
      bind:this={panel}
      role="dialog"
      aria-modal="true"
      aria-labelledby="{id}-title"
      aria-describedby={description ? `${id}-desc` : undefined}
      tabindex="-1"
      {onkeydown}
      class="relative flex max-h-full w-full flex-col overflow-hidden rounded-sheet border border-border bg-elevated text-fg shadow-[var(--elev-3)] outline-none"
      style:max-width="{width}px"
      in:pop={{ from: 0.95, duration: 240, origin }}
      out:unpop={{ duration: 140, origin }}
    >
      <header class="flex items-start gap-3 px-5 pb-2 pt-4">
        <div class="min-w-0 flex-1">
          <h2 id="{id}-title" class="text-title-3">{title}</h2>
          {#if description}<p id="{id}-desc" class="mt-0.5 text-callout text-fg-muted">{description}</p>{/if}
        </div>
        <IconButton icon={X} label="Close" chord="Escape" size="sm" onclick={close} />
      </header>
      <div class="scroll-room min-h-0 flex-1 overflow-y-auto px-5 pb-4 pt-1">
        {@render children()}
      </div>
      {#if footer}
        <footer class="flex items-center justify-end gap-2 border-t border-border px-5 py-3">
          {@render footer()}
        </footer>
      {/if}
    </div>
  </div>
{/if}
