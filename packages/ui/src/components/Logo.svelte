<script lang="ts">
  /**
   * The Shotmaxxing mark: a lens aperture. Six blade edges run from the hexagonal opening out to the barrel.
   * `animate` opens the aperture once (used on the welcome screen).
   */
  let {
    size = 24,
    wordmark = false,
    tile = false,
    animate = false,
  }: { size?: number; wordmark?: boolean; tile?: boolean; animate?: boolean } = $props();

  const blades = [0, 60, 120, 180, 240, 300];
</script>

<span class="inline-flex items-center gap-2">
  <span
    class={[
      'inline-grid shrink-0 place-items-center',
      tile && 'rounded-[28%] bg-accent text-accent-fg shadow-[var(--top-light)]',
    ]}
    style:width="{size}px"
    style:height="{size}px"
  >
    <svg
      width={tile ? size * 0.66 : size}
      height={tile ? size * 0.66 : size}
      viewBox="-12 -12 24 24"
      fill="none"
      aria-hidden="true"
      class={animate ? 'aperture-open' : ''}
    >
      <circle r="10" stroke="currentColor" stroke-width="1.9" />
      {#each blades as angle (angle)}
        <path
          d="M0 -3.6 L9.79 2.05"
          stroke="currentColor"
          stroke-width="1.9"
          stroke-linecap="round"
          transform="rotate({angle})"
        />
      {/each}
    </svg>
  </span>
  {#if wordmark}
    <span class="text-[15px] font-semibold tracking-[-0.02em]" style:font-size="{Math.max(13, size * 0.62)}px"
      >Shotmaxxing</span
    >
  {:else}
    <span class="sr-only">Shotmaxxing</span>
  {/if}
</span>

<style>
  .aperture-open {
    animation: aperture 640ms var(--ease-out) both; /* design-guard-allow long: plays once on the welcome screen */
  }
  @keyframes aperture {
    from {
      rotate: -60deg;
      scale: 0.82;
      opacity: 0;
    }
  }
  :global(:root[data-motion='fades']) .aperture-open {
    animation-name: fade;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
</style>
