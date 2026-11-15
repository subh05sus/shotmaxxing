<script lang="ts">
  /**
   * A small picture of what a permission lets Shotmaxxing do, acted out once when it appears and again when you
   * point at its row: Screen Recording frames the screen with a REC light, the microphone shows a level, the camera
   * puts you in a bubble, Accessibility follows a click and zooms to it. Each rests on its last frame, so nothing
   * moves while you read; with reduced motion it shows that frame straight away.
   */
  import type { PermissionKind } from '../../lib/native';

  let { kind, play = 0, delay = 0 }: { kind: PermissionKind; play?: number; delay?: number } = $props();
</script>

{#key play}
  <svg
    viewBox="0 0 72 52"
    class="art h-[52px] w-[72px] shrink-0 rounded-[12px] bg-fill"
    style:--delay="{delay}ms"
    aria-hidden="true"
  >
    {#if kind === 'screen' || kind === 'camera'}
      <rect
        x="10"
        y="7"
        width="52"
        height="33"
        rx="4"
        class="fill-surface stroke-fg-subtle"
        stroke-width="1.4"
      />
      <rect x="29" y="40" width="14" height="4" rx="1.5" class="fill-fg-subtle" />
      <rect x="16" y="13" width="31" height="21" rx="2.5" class="fill-fg-subtle" opacity="0.32" />
    {/if}
    {#if kind === 'screen'}
      <rect
        class="frame stroke-accent"
        x="14"
        y="11"
        width="35"
        height="25"
        rx="3.5"
        fill="none"
        stroke-width="1.6"
        pathLength="100"
        stroke-dasharray="100"
      />
      <circle class="rec fill-record" cx="54.5" cy="13" r="2.6" />
    {:else if kind === 'camera'}
      <g class="bubble">
        <circle cx="51" cy="30" r="9" class="fill-surface stroke-accent" stroke-width="1.6" />
        <circle cx="51" cy="27.5" r="3" class="fill-fg-muted" />
        <path d="M44.6 36.2a6.6 5.4 0 0 1 12.8 0" class="fill-fg-muted" />
      </g>
    {:else if kind === 'microphone'}
      <rect
        x="15"
        y="11"
        width="11"
        height="19"
        rx="5.5"
        class="fill-surface stroke-fg-subtle"
        stroke-width="1.4"
      />
      <path
        d="M12 24a8.5 8.5 0 0 0 17 0M20.5 32.5V38M16 38h9"
        fill="none"
        class="stroke-fg-subtle"
        stroke-width="1.4"
        stroke-linecap="round"
      />
      {#each [0, 1, 2, 3, 4] as bar (bar)}
        <rect
          class="bar fill-accent"
          x={36 + bar * 6}
          y="12"
          width="3.2"
          height="26"
          rx="1.6"
          style:--bar={bar}
        />
      {/each}
    {:else}
      <g class="zoom">
        <rect
          x="12"
          y="8"
          width="48"
          height="35"
          rx="4"
          class="fill-surface stroke-fg-subtle"
          stroke-width="1.4"
        />
        <rect x="17" y="14" width="18" height="3" rx="1.5" class="fill-fg-subtle" opacity="0.5" />
        <rect x="17" y="20" width="26" height="3" rx="1.5" class="fill-fg-subtle" opacity="0.3" />
        <rect x="34" y="29" width="18" height="8" rx="3" class="fill-accent" />
        <circle class="ripple stroke-accent" cx="43" cy="33" r="7" fill="none" stroke-width="1.4" />
        <path
          class="cursor fill-fg stroke-surface"
          d="M42 32v10.5l2.6-2.4 2 4.4 1.7-.8-2-4.3h3.6z"
          stroke-width="0.9"
          stroke-linejoin="round"
        />
      </g>
    {/if}
  </svg>
{/key}

<style>
  /* Every picture's resting state is its last frame; the animations run from the first one into it. */
  .frame {
    animation: draw 900ms var(--ease-out) var(--delay) both; /* design-guard-allow long: plays once */
  }
  @keyframes draw {
    from {
      stroke-dashoffset: 100;
    }
    to {
      stroke-dashoffset: 0;
    }
  }
  .rec {
    animation: rec 1600ms step-end calc(var(--delay) + 500ms) both; /* design-guard-allow long: two blinks, once */
  }
  @keyframes rec {
    0% {
      opacity: 0;
    }
    12%,
    38%,
    66%,
    100% {
      opacity: 1;
    }
    25%,
    52% {
      opacity: 0.25;
    }
  }
  .bubble {
    transform-box: fill-box;
    transform-origin: center;
    animation: bubble 520ms var(--ease-out) calc(var(--delay) + 250ms) both; /* design-guard-allow long: once */
  }
  @keyframes bubble {
    0% {
      transform: scale(0);
      opacity: 0;
    }
    60% {
      transform: scale(1.12);
      opacity: 1;
    }
    100% {
      transform: scale(1);
    }
  }
  .bar {
    transform-box: fill-box;
    transform-origin: center;
    transform: scaleY(calc(0.26 + 0.06 * var(--bar)));
    animation-name: level;
    animation-duration: calc(1100ms + 90ms * var(--bar)); /* design-guard-allow long: twice, then rests */
    animation-timing-function: ease-in-out;
    animation-delay: calc(var(--delay) + 60ms * var(--bar));
    animation-iteration-count: 2;
    animation-fill-mode: both;
  }
  @keyframes level {
    0% {
      transform: scaleY(0.2);
    }
    20% {
      transform: scaleY(0.95);
    }
    40% {
      transform: scaleY(0.45);
    }
    62% {
      transform: scaleY(1);
    }
    82% {
      transform: scaleY(0.55);
    }
    100% {
      transform: scaleY(calc(0.26 + 0.06 * var(--bar)));
    }
  }
  .cursor {
    animation: point 760ms var(--ease-out) var(--delay) both; /* design-guard-allow long: once */
  }
  @keyframes point {
    from {
      transform: translate(-20px, -16px);
      opacity: 0;
    }
    30% {
      opacity: 1;
    }
  }
  .ripple {
    transform-box: fill-box;
    transform-origin: center;
    opacity: 0;
    animation: ripple 560ms var(--ease-out) calc(var(--delay) + 800ms) both; /* design-guard-allow long: once */
  }
  @keyframes ripple {
    from {
      transform: scale(0.3);
      opacity: 0.9;
    }
    to {
      transform: scale(1.4);
      opacity: 0;
    }
  }
  .zoom {
    transform-origin: 43px 33px;
    animation: zoom 1500ms var(--ease-out) calc(var(--delay) + 950ms) both; /* design-guard-allow long: in and back out, once */
  }
  @keyframes zoom {
    0%,
    100% {
      transform: scale(1);
    }
    35%,
    70% {
      transform: scale(1.22);
    }
  }
  :global(:root[data-motion='fades']) .art * {
    animation: none;
  }
</style>
