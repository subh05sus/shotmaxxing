<script lang="ts">
  import { Ai, Bot, Clapperboard, Copy, Hand, Lock, Play, ShieldCheck, Wand } from '@shotmaxxing/ui/icons';
  import { Button, CheckDraw, Group, Icon, Row, StatusDot, env, toast } from '@shotmaxxing/ui';
  import { onMount } from 'svelte';
  import { STARTER_PROMPTS, type Agent } from '../../lib/sample';
  import { connectAgent, disconnectAgent, mcpCommand, openWindow } from '../../lib/native';
  import { go } from '../../lib/router.svelte';
  import { agents, connectProblem, refreshAgents, type AgentEntry } from '../../lib/agents.svelte';
  import { trust } from '../../lib/trust.svelte';
  import AllowEverything from './AllowEverything.svelte';

  let command = $state('shotmaxxing');
  onMount(() => {
    void refreshAgents();
    void mcpCommand().then((path) => (command = path));
  });

  async function connect(agent: AgentEntry) {
    agent.busy = true;
    const result = await connectAgent(agent.id).catch((error) => ({
      ok: false,
      message: `Couldn’t connect: ${error}`,
    }));
    agent.busy = false;
    if (!result.ok) return connectProblem(result);
    agent.state = 'connected';
    toast(`${agent.name} can now film with Shotmaxxing. Start a new chat to use it.`, { tone: 'success' });
    void refreshAgents();
  }

  async function disconnect(agent: AgentEntry) {
    agent.busy = true;
    const result = await disconnectAgent(agent.id).catch((error) => ({
      ok: false,
      message: `Couldn’t disconnect: ${error}`,
    }));
    agent.busy = false;
    if (!result.ok) return connectProblem(result);
    agent.state = 'available';
    toast(`Disconnected ${agent.name}`, { action: { label: 'Undo', run: () => void connect(agent) } });
    void refreshAgents();
  }

  async function copy(text: string, what = 'Copied') {
    try {
      await navigator.clipboard.writeText(text);
      toast(what);
    } catch {
      toast('Couldn’t reach the clipboard. Select the text and copy it instead.', { tone: 'danger' });
    }
  }

  const config = $derived(`{
  "mcpServers": {
    "shotmaxxing": { "command": ${JSON.stringify(command)}, "args": ["mcp"] }
  }
}`);

  const machine = env.os === 'mac' ? 'Mac' : 'PC';
</script>

<div class="scroll-room h-full overflow-y-auto px-5 pb-28">
  <p class="measure text-callout text-fg-muted">
    Let Claude, Codex and other agents film demos for you. They plan the take, perform it in one smooth pass
    and hand you an editable project. Everything runs on this {machine}.
  </p>

  <div class="mt-5 grid gap-6 lg:grid-cols-[minmax(0,1fr)_320px]">
    <div class="flex min-w-0 flex-col gap-6">
      <Group title="Connect">
        {#each agents as agent (agent.id)}
          <div class="flex min-h-14 items-center gap-3 py-2">
            <span class="grid size-8 shrink-0 place-items-center rounded-[9px] bg-fill text-fg-muted"
              ><Icon icon={Bot} size={16} aria-hidden="true" /></span
            >
            <div class="min-w-0 flex-1">
              <p class="text-callout font-semibold">{agent.name}</p>
              <p class="text-footnote text-fg-muted">
                {agent.kind}{#if agent.lastTake}{' · '}last take {agent.lastTake}{/if}
              </p>
            </div>
            {#if agent.state === 'connected'}
              <StatusDot tone="success" label="Connected" />
              <Button size="sm" variant="ghost" loading={agent.busy} onclick={() => disconnect(agent)}
                >Disconnect</Button
              >
            {:else if agent.state === 'not_installed'}
              <StatusDot tone="neutral" label="Not installed" />
            {:else}
              <Button size="sm" loading={agent.busy} onclick={() => connect(agent)}>Connect</Button>
            {/if}
          </div>
        {/each}
      </Group>

      <Group
        title="Safety"
        footer="An agent can always see these rules. Stop on the take card, pressing Esc three times, or moving the mouse ends any take."
      >
        <Row
          label="Ask before an agent sees the screen"
          description="Once each agent session, on the take card, which names the program that’s asking."
        >
          <StatusDot tone={trust.on ? 'danger' : 'success'} label={trust.on ? 'Skipped' : 'On'} />
        </Row>
        <Row
          label="Ask before every take"
          description="The take card shows the plan and each step before anything moves."
        >
          <StatusDot tone={trust.on ? 'danger' : 'success'} label={trust.on ? 'Skipped' : 'On'} />
        </Row>
        {#each [{ label: 'Never type into password fields', description: 'Agents hand those steps to you.' }, { label: 'Show the live take card', description: 'Each step as the agent performs it, with Pause, Take over and Stop.' }] as rule (rule.label)}
          <Row label={rule.label} description={rule.description}>
            <span class="flex items-center gap-1.5 text-footnote text-fg-muted"
              ><Icon icon={Lock} size={13} aria-hidden="true" /> Always on</span
            >
          </Row>
        {/each}
        <AllowEverything />
      </Group>

      <Group
        title="Set up by hand"
        footer="For agents that aren’t listed: add this to the agent’s MCP settings, then restart it."
      >
        <div class="relative py-3">
          <pre
            class="selectable overflow-x-auto rounded-[8px] bg-fill p-3 font-mono text-footnote leading-[1.5]">{config}</pre>
          <div class="absolute right-2 top-5">
            <Button size="sm" icon={Copy} onclick={() => copy(config, 'Copied the MCP settings')}>Copy</Button
            >
          </div>
        </div>
      </Group>
    </div>

    <aside class="flex flex-col gap-6 lg:pt-[calc(var(--text-caption--line-height)+6px)]">
      <section class="rounded-card border border-border bg-surface p-4 shadow-[var(--elev-1)]">
        <h3 class="flex items-center gap-2 text-callout font-semibold">
          <Icon icon={Ai} size={15} class="text-accent-text" aria-hidden="true" /> Try asking
        </h3>
        <ul class="mt-3 flex flex-col gap-2">
          {#each STARTER_PROMPTS as prompt (prompt)}
            <li>
              <button
                type="button"
                class="press-card group flex w-full items-start gap-2 rounded-[8px] bg-fill p-2.5 text-left text-footnote leading-[1.45] hover:bg-fill-hover"
                onclick={() => copy(prompt, 'Prompt copied. Paste it into your agent.')}
              >
                <span class="flex-1">“{prompt}”</span>
                <Icon
                  icon={Copy}
                  size={13}
                  class="mt-0.5 shrink-0 text-fg-subtle group-hover:text-fg-muted"
                  aria-hidden="true"
                />
              </button>
            </li>
          {/each}
        </ul>
      </section>

      <section class="rounded-card border border-border bg-surface p-4 shadow-[var(--elev-1)]">
        <h3 class="text-callout font-semibold">How a take works</h3>
        <ol class="mt-3 flex flex-col gap-3">
          {#each [{ icon: Wand, title: 'Plan', body: 'The agent learns the flow from your code, shows you a plan in plain steps and checks each one on screen.' }, { icon: Clapperboard, title: 'Perform', body: 'One smooth take with a natural cursor. Watch it live, pause it or take over.' }, { icon: Hand, title: 'Hand off', body: 'Sign-ins and passwords come to you. The agent waits until you hand back.' }, { icon: ShieldCheck, title: 'Polish', body: 'The take opens in the editor with zooms already added, ready to export.' }] as stepItem, i (stepItem.title)}
            <li class="flex gap-3">
              <span
                class="grid size-7 shrink-0 place-items-center rounded-full bg-accent-tint text-accent-text"
                ><Icon icon={stepItem.icon} size={14} aria-hidden="true" /></span
              >
              <span>
                <span class="block text-footnote font-semibold"
                  ><span class="tabular">{i + 1}.</span> {stepItem.title}</span
                >
                <span class="block text-footnote text-fg-muted">{stepItem.body}</span>
              </span>
            </li>
          {/each}
        </ol>
        <div class="mt-4 flex gap-2">
          <Button size="sm" icon={Play} onclick={() => openWindow('take')}>Preview the take card</Button>
          <Button size="sm" variant="ghost" onclick={() => go('home/learn')}>Guides</Button>
        </div>
      </section>

      <p class="flex items-center gap-1.5 px-1 text-footnote text-fg-muted">
        <CheckDraw size={14} class="text-success" /> Nothing is uploaded. Agents talk to Shotmaxxing on this {machine}
        only.
      </p>
    </aside>
  </div>
</div>
