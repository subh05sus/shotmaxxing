<script lang="ts">
  // What changed in some versions, newest first, as the changelog writes it.
  import { readableDate, type Span, type Version } from '../../lib/changelog';
  import { openLink } from '../../lib/native';

  let { versions }: { versions: Version[] } = $props();
</script>

{#snippet line(spans: Span[])}
  {#each spans as span, i (i)}
    {#if span.kind === 'bold'}<strong class="font-semibold text-fg">{span.text}</strong>
    {:else if span.kind === 'code'}<code class="rounded-[4px] bg-fill px-1 font-mono text-[0.9em]"
        >{span.text}</code
      >
    {:else if span.kind === 'link'}<button
        type="button"
        class="text-accent-text underline-offset-2 hover:underline"
        onclick={() => void openLink(span.href)}>{span.text}</button
      >
    {:else}{span.text}{/if}
  {/each}
{/snippet}

<div class="flex flex-col gap-5">
  {#each versions as v (v.version)}
    <section aria-labelledby="notes-{v.version}">
      <h3 id="notes-{v.version}" class="flex items-baseline gap-2">
        <span class="text-headline">{v.version}</span>
        {#if v.date}<span class="text-footnote text-fg-subtle">{readableDate(v.date)}</span>{/if}
      </h3>
      {#each v.blocks as block, i (i)}
        {#if block.kind === 'heading'}
          <p class="eyebrow mt-3">{block.text}</p>
        {:else if block.kind === 'paragraph'}
          <p class="mt-1.5 text-callout leading-relaxed text-fg-muted">{@render line(block.spans)}</p>
        {:else}
          <ul class="mt-2 flex flex-col gap-2">
            {#each block.items as item, j (j)}
              <li class="flex gap-2.5 text-callout leading-relaxed text-fg-muted">
                <span class="mt-[0.6em] size-1.5 shrink-0 rounded-full bg-accent" aria-hidden="true"></span>
                <span class="min-w-0">{@render line(item)}</span>
              </li>
            {/each}
          </ul>
        {/if}
      {/each}
    </section>
  {/each}
</div>
