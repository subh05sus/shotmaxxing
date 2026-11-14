<script lang="ts">
  /**
   * Digits roll to their new value, each column sliding with transform only. Tabular figures keep the width steady.
   * For values that change every frame (a recording timer) use plain tabular text instead: frequent things shouldn't move.
   */
  let {
    value,
    format = (v: number) => String(v),
    class: className = '',
  }: { value: number; format?: (v: number) => string; class?: string } = $props();

  const text = $derived(format(value));
  const chars = $derived([...text]);
</script>

<span class={['inline-flex overflow-hidden tabular leading-none', className]}>
  <span class="sr-only">{text}</span>
  {#each chars as ch, i (chars.length - i)}
    {#if ch >= '0' && ch <= '9'}
      <span class="relative inline-block h-[1em] w-[1ch] overflow-hidden" aria-hidden="true">
        <span
          class="absolute left-0 top-0 flex flex-col transition-[translate] duration-[var(--dur-slow)] ease-[var(--ease-spring)]"
          style:translate="0 {-Number(ch)}em"
        >
          {#each [0, 1, 2, 3, 4, 5, 6, 7, 8, 9] as d (d)}<span class="h-[1em] leading-none">{d}</span>{/each}
        </span>
      </span>
    {:else}
      <span aria-hidden="true">{ch}</span>
    {/if}
  {/each}
</span>
