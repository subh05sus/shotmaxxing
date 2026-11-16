<script lang="ts">
  import { ShieldCheck } from '@shotmaxxing/ui/icons';
  import { Button, Icon, Row, Sheet, Switch } from '@shotmaxxing/ui';
  import { confirmTrust, trust, turnTrust } from '../../lib/trust.svelte';

  /**
   * Allow everything, in Settings › Agents and on the Agents page: a red switch that lets agents film without the
   * take card asking. The first time it's turned on, a sheet says what it skips and what still holds.
   */

  const KEPT = [
    'The take card follows every take, with Pause, Take over and Stop.',
    'Pressing Esc three times, or moving the mouse, stops a take at once.',
    'Takes never type into password fields: those steps come to you.',
    'A red edge goes round the screen while an agent moves the pointer and types.',
  ];
</script>

<Row
  label="Allow everything"
  description="Agents film without asking you. Esc three times or moving the mouse still stops a take. Turns off when you quit Shotmaxxing."
>
  <Switch tone="danger" label="Allow everything" bind:checked={() => trust.on, (on) => turnTrust(on)} />
</Row>

<Sheet
  bind:open={trust.confirming}
  title="Let agents film without asking?"
  description="Connected agents will see your screen and film takes without the take card asking you first, until you turn this off or quit Shotmaxxing."
  width={480}
>
  <p class="eyebrow">Still on</p>
  <ul class="mt-2 flex flex-col gap-2">
    {#each KEPT as line (line)}
      <li class="flex items-start gap-2 text-callout leading-[1.45]">
        <Icon icon={ShieldCheck} size={15} class="mt-0.5 shrink-0 text-success" aria-hidden="true" />
        <span>{line}</span>
      </li>
    {/each}
  </ul>
  <p class="mt-4 text-footnote leading-[1.45] text-fg-muted">
    Agents get through by way of the shotmaxxing command, and so would anything else on this computer that
    runs it. Turn it on only while you trust what’s running.
  </p>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (trust.confirming = false)}>Cancel</Button>
    <Button variant="danger" onclick={confirmTrust}>Allow everything</Button>
  {/snippet}
</Sheet>
