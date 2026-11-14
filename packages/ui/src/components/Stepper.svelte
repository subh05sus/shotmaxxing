<script lang="ts">
  import CheckDraw from './CheckDraw.svelte';

  /** Onboarding progress: finished steps draw a check, the current one is filled, the rest wait. */
  let { steps, current, onselect }: { steps: string[]; current: number; onselect?: (index: number) => void } =
    $props();
</script>

<ol class="flex items-center gap-1.5" aria-label="Setup progress">
  {#each steps as step, i (step)}
    <li class="flex items-center gap-1.5">
      <button
        type="button"
        disabled={!onselect || i > current}
        onclick={() => onselect?.(i)}
        aria-current={i === current ? 'step' : undefined}
        class="press group flex items-center gap-1.5 rounded-full py-1 pl-1 pr-2 enabled:hover:bg-fill disabled:cursor-default"
      >
        <span
          class={[
            'grid size-5 place-items-center rounded-full text-caption font-semibold transition-colors duration-[var(--dur-base)]',
            i < current && 'bg-accent text-accent-fg',
            i === current && 'bg-fg text-bg',
            i > current && 'bg-fill text-fg-subtle',
          ]}
        >
          {#if i < current}<CheckDraw size={12} />{:else}<span class="tabular">{i + 1}</span>{/if}
        </span>
        <span class={['text-footnote font-medium', i === current ? 'text-fg' : 'text-fg-subtle']}>{step}</span
        >
      </button>
      {#if i < steps.length - 1}
        <span
          class={[
            'h-px w-5 transition-colors duration-[var(--dur-base)]',
            i < current ? 'bg-accent' : 'bg-border-strong',
          ]}
          aria-hidden="true"
        ></span>
      {/if}
    </li>
  {/each}
</ol>
