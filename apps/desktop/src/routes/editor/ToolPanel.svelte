<script lang="ts">
  import {
    Ai,
    ArrowDownLeft,
    ArrowDownRight,
    ArrowUpLeft,
    ArrowUpRight,
    Blur,
    BookOpen,
    Captions,
    Lock,
    Mic,
    Music,
    RefreshCw,
    Type,
    WandSparkles,
    X,
  } from '@shotmaxxing/ui/icons';
  import {
    Advanced,
    Button,
    Icon,
    IconButton,
    InfoTip,
    Keycap,
    ProgressRing,
    Segmented,
    Slider,
    Spinner,
    StatusDot,
    Switch,
  } from '@shotmaxxing/ui';
  import { languageName, width } from '../../lib/captions';
  import { timecode } from '../../lib/format';
  import { settings } from '../../lib/settings.svelte';
  import { WALLPAPERS } from '../../lib/wallpapers';
  import { go } from '../../lib/router.svelte';
  import type { EditorState, ToolId } from './store.svelte';

  let { ed, onhelp }: { ed: EditorState; onhelp: (tool: ToolId) => void } = $props();

  const p = $derived(ed.project);

  const TITLES: Record<ToolId, { title: string; blurb: string; guide?: string }> = {
    background: { title: 'Background', blurb: 'What sits behind your recording.' },
    cursor: { title: 'Cursor', blurb: 'Make every move easy to follow.', guide: 'cursor' },
    zoom: { title: 'Zoom', blurb: 'Push in where the action is.', guide: 'auto-zoom' },
    annotate: { title: 'Annotate', blurb: 'Titles, arrows and blur.', guide: 'blur' },
    camera: { title: 'Camera', blurb: 'You, in a bubble.', guide: 'record-camera' },
    audio: { title: 'Audio', blurb: 'Voice, system sound, voice-overs and music.', guide: 'voice-over' },
    ai: { title: 'AI helpers', blurb: 'Captions, cleanup and cuts, made on this computer.' },
  };

  const info = $derived(
    ed.still && ed.tool === 'background'
      ? { ...TITLES.background, blurb: 'What sits behind your screenshot.', guide: 'screenshot' }
      : TITLES[ed.tool],
  );
  const pct = (v: number) => `${Math.round(v * 100)}%`;

  const silenceSeconds = $derived(p.audio.silences.reduce((sum, s) => sum + s.end - s.start, 0));

  // Captions: made from the take's microphone and any voice-overs.
  const voice = $derived(p.has.mic || p.audio.voiceOvers.length > 0);
  const captionJob = $derived(ed.captionJob);
  const captionNow = $derived(ed.captionNow?.line.id);
  const modelMb = $derived(Math.round((ed.captionModel?.bytes ?? 59_707_625) / 1e6));
  const jobText = $derived.by(() => {
    const job = ed.captionJob;
    if (!job) return '';
    const pct = `${Math.round(job.progress * 100)}%`;
    switch (job.stage) {
      case 'starting':
        return 'Getting ready…';
      case 'waiting':
        return 'Waiting for another project’s captions…';
      case 'download':
        return `Downloading the speech model… ${pct}`;
      case 'listen':
        return 'Listening to the voice…';
      case 'transcribe':
        return `Transcribing… ${pct}`;
    }
  });
  // Whether captions can be made here is asked once, when this panel first opens.
  $effect(() => {
    if (ed.tool === 'ai' && !ed.captionModel) void ed.checkCaptionModel();
  });

  /** A message from the app as a sentence. */
  const sentence = (text: string) =>
    `${text.charAt(0).toUpperCase()}${text.slice(1)}${/[.!?]$/.test(text) ? '' : '.'}`;
</script>

<aside
  class="flex w-[288px] shrink-0 flex-col border-r border-border bg-surface"
  aria-label="{info.title} settings"
>
  <header class="flex items-start gap-2 px-4 pb-2 pt-3.5">
    <div class="min-w-0 flex-1">
      <h2 class="text-title-3">{info.title}</h2>
      <p class="text-footnote text-fg-muted">{info.blurb}</p>
    </div>
    <button
      type="button"
      class="press-icon -mr-1 grid size-7 place-items-center rounded-[7px] text-fg-subtle hover:bg-fill hover:text-fg"
      aria-label="Show me how {info.title} works"
      title="Show me how this works"
      onclick={() => onhelp(ed.tool)}
    >
      <Icon icon={BookOpen} size={15} aria-hidden="true" />
    </button>
  </header>

  <div class="scroll-room flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto px-4 pb-4" data-tour="panel">
    {#if ed.tool === 'background'}
      <p class="eyebrow mt-1">Wallpaper</p>
      <div class="grid grid-cols-3 gap-2 py-1" role="radiogroup" aria-label="Wallpaper">
        {#each WALLPAPERS as w (w.id)}
          <button
            type="button"
            role="radio"
            aria-checked={p.canvas.wallpaper === w.id}
            title={w.name}
            aria-label={w.name}
            class={[
              'press-card aspect-[4/3] rounded-[8px] shadow-[var(--elev-1)]',
              p.canvas.wallpaper === w.id && 'ring-2 ring-accent ring-offset-2 ring-offset-surface',
            ]}
            style:background={w.css}
            onclick={() => ed.change('Wallpaper', () => (p.canvas.wallpaper = w.id))}
          ></button>
        {/each}
      </div>
      <Slider
        label="Padding"
        value={p.canvas.padding}
        min={0}
        max={0.2}
        step={0.005}
        format={pct}
        onchange={(v) => ed.change('Padding', () => (p.canvas.padding = v))}
      />
      <Slider
        label="Corner radius"
        value={p.canvas.radius}
        min={0}
        max={40}
        step={1}
        format={(v) => `${v} px`}
        onchange={(v) => ed.change('Radius', () => (p.canvas.radius = v))}
      />
      <Slider
        label="Shadow"
        value={p.canvas.shadow}
        min={0}
        max={1}
        step={0.05}
        format={pct}
        onchange={(v) => ed.change('Shadow', () => (p.canvas.shadow = v))}
      />
      <p class="eyebrow mt-2">Size</p>
      <Segmented
        label="Aspect ratio"
        size="sm"
        value={p.canvas.aspect}
        onchange={(v) => ed.change('Aspect ratio', () => (p.canvas.aspect = v))}
        options={[
          ...(ed.still ? [{ value: 'original' as const, label: 'Fit' }] : []),
          { value: '16:9', label: '16:9' },
          { value: '4:3', label: '4:3' },
          { value: '1:1', label: '1:1' },
          { value: '9:16', label: '9:16' },
        ]}
      />
    {:else if ed.tool === 'cursor'}
      <Slider
        label="Size"
        value={p.cursor.size}
        min={1}
        max={3}
        step={0.1}
        format={(v) => `${v.toFixed(1)}×`}
        onchange={(v) => ed.change('Cursor size', () => (p.cursor.size = v))}
      />
      <Slider
        label="Smoothing"
        value={p.cursor.smoothing}
        min={0}
        max={1}
        step={0.05}
        format={pct}
        info="Irons out shaky movement. The cursor still lands exactly where you clicked."
        onchange={(v) => ed.change('Smoothing', () => (p.cursor.smoothing = v))}
      />
      <p class="eyebrow mt-2">Style</p>
      <Segmented
        label="Cursor style"
        size="sm"
        value={p.cursor.style}
        onchange={(v) => ed.change('Cursor style', () => (p.cursor.style = v))}
        options={[
          { value: 'mac', label: 'macOS' },
          { value: 'windows', label: 'Windows' },
          { value: 'dot', label: 'Dot' },
        ]}
      />
      <p class="eyebrow mt-3">Clicks</p>
      <Segmented
        label="Click effect"
        size="sm"
        value={p.cursor.click}
        onchange={(v) => ed.change('Click effect', () => (p.cursor.click = v))}
        options={[
          { value: 'ripple', label: 'Ripple' },
          { value: 'press', label: 'Press' },
          { value: 'none', label: 'None' },
        ]}
      />
      <div class="mt-1 flex flex-col">
        <label class="flex h-9 items-center justify-between text-callout">
          <span class="flex items-center gap-1"
            >Snap to clicks <InfoTip
              text="Near misses land on the button you meant, so clicks look deliberate."
            /></span
          >
          <Switch
            checked={p.cursor.snap}
            label="Snap to clicks"
            onchange={(v) => ed.change('Snap', () => (p.cursor.snap = v))}
          />
        </label>
        <label class="flex h-9 items-center justify-between text-callout">
          Click sounds
          <Switch
            checked={p.cursor.sounds}
            label="Click sounds"
            onchange={(v) => ed.change('Click sounds', () => (p.cursor.sounds = v))}
          />
        </label>
      </div>
      <Advanced id="cursor">
        <label class="flex h-9 items-center justify-between text-callout">
          Motion blur
          <Switch
            checked={p.cursor.motionBlur}
            label="Motion blur"
            onchange={(v) => ed.change('Motion blur', () => (p.cursor.motionBlur = v))}
          />
        </label>
        <label class="flex h-9 items-center justify-between text-callout">
          Hide when still
          <Switch
            checked={p.cursor.hideIdle}
            label="Hide when still"
            onchange={(v) => ed.change('Hide idle', () => (p.cursor.hideIdle = v))}
          />
        </label>
        <label class="flex h-9 items-center justify-between text-callout">
          <span class="flex items-center gap-1"
            >Spotlight <InfoTip text="A soft light around the cursor, for busy screens." /></span
          >
          <Switch
            checked={p.cursor.spotlight}
            label="Spotlight"
            onchange={(v) => ed.change('Spotlight', () => (p.cursor.spotlight = v))}
          />
        </label>
        <label class="flex h-9 items-center justify-between text-callout">
          <span class="flex items-center gap-1"
            >Loop to start <InfoTip text="Ends where it began, so GIFs loop without a jump." /></span
          >
          <Switch
            checked={p.cursor.loop}
            label="Loop to start"
            onchange={(v) => ed.change('Loop', () => (p.cursor.loop = v))}
          />
        </label>
      </Advanced>
    {:else if ed.tool === 'zoom'}
      <label class="flex h-10 items-center justify-between text-callout font-medium">
        Auto zoom
        <Switch checked={p.zoom.auto} label="Auto zoom" onchange={(v) => void ed.setAutoZoom(v)} />
      </label>
      {#if !p.zoom.auto}
        <p class="text-footnote text-fg-muted">
          Only zooms you add play. Press <Keycap chord="z" /> to add one at the playhead.
        </p>
      {:else}
        <p class="eyebrow mt-1">How far</p>
        <Segmented
          label="Zoom depth"
          size="sm"
          value={p.zoom.depth}
          onchange={(v) => {
            ed.change('Zoom depth', () => (p.zoom.depth = v));
            void ed.redoAutoZoom('Zoom depth');
          }}
          options={[
            { value: 'subtle', label: 'Subtle' },
            { value: 'standard', label: 'Standard' },
            { value: 'bold', label: 'Bold' },
          ]}
        />
      {/if}
      <p class="eyebrow mt-3">Motion</p>
      <Segmented
        label="Zoom motion"
        size="sm"
        value={p.zoom.motion}
        onchange={(v) => ed.change('Zoom motion', () => (p.zoom.motion = v))}
        options={[
          { value: 'smooth', label: 'Smooth' },
          { value: 'instant', label: 'Instant' },
        ]}
      />
      <label class="mt-1 flex h-9 items-center justify-between text-callout">
        <span class="flex items-center gap-1"
          >Follow the cursor <InfoTip
            text="While zoomed in, the view drifts with the cursor instead of holding still."
          /></span
        >
        <Switch
          checked={p.zoom.follow}
          label="Follow the cursor"
          onchange={(v) => ed.change('Follow', () => (p.zoom.follow = v))}
        />
      </label>
      {#if p.zoom.auto}
        <Button size="sm" icon={RefreshCw} onclick={() => ed.redoAutoZoom()}>Redo auto zoom</Button>
        <p class="mt-1 text-footnote text-fg-muted">Keeps zooms you added or changed by hand.</p>
      {/if}

      <p class="eyebrow mt-4">In this video <span class="tabular">· {p.zooms.length}</span></p>
      <ul class="flex flex-col gap-1 py-1">
        {#each [...p.zooms].sort((a, b) => a.start - b.start) as z (z.id)}
          <li>
            <button
              type="button"
              class={[
                'ring-inside flex w-full items-center gap-2 rounded-[7px] px-2 py-1.5 text-left',
                ed.selection?.id === z.id ? 'bg-fill-strong' : 'hover:bg-fill',
              ]}
              onclick={() => {
                ed.selection = { kind: 'zoom', id: z.id };
                ed.seek(z.start + 0.8);
              }}
            >
              <span class="w-9 text-footnote font-semibold text-accent-text tabular"
                >{z.scale.toFixed(1)}×</span
              >
              <span class="min-w-0 flex-1 truncate text-footnote">{z.reason}</span>
              {#if z.source === 'manual'}<span class="text-caption text-fg-subtle">by you</span>{/if}
            </button>
          </li>
        {/each}
      </ul>
      {#if p.zoom.auto}
        <Advanced id="zoom">
          <Slider
            label="Hold after a click"
            value={p.zoom.hold}
            min={0.4}
            max={3}
            step={0.1}
            format={(v) => `${v.toFixed(1)} s`}
            onchange={(v) => {
              ed.change('Hold', () => (p.zoom.hold = v));
              ed.replanSoon('Hold');
            }}
          />
          <Slider
            label="Merge moments closer than"
            value={p.zoom.mergeGap}
            min={0}
            max={4}
            step={0.1}
            format={(v) => `${v.toFixed(1)} s`}
            info="Nearby zooms glide into each other instead of zooming out and back in."
            onchange={(v) => {
              ed.change('Merge gap', () => (p.zoom.mergeGap = v));
              ed.replanSoon('Merge gap');
            }}
          />
          <label class="flex h-9 items-center justify-between text-callout">
            Zoom while typing
            <Switch
              checked={p.zoom.typing}
              label="Zoom while typing"
              onchange={(v) => {
                ed.change('Typing zooms', () => (p.zoom.typing = v));
                ed.replanSoon('Typing zooms');
              }}
            />
          </label>
          <label class="flex h-9 items-center justify-between text-callout">
            <span class="flex items-center gap-1"
              >Zoom on a pause <InfoTip
                text="When the cursor rests on something for a second, it is probably worth a closer look."
              /></span
            >
            <Switch
              checked={p.zoom.dwell}
              label="Zoom on a pause"
              onchange={(v) => {
                ed.change('Dwell zooms', () => (p.zoom.dwell = v));
                ed.replanSoon('Dwell zooms');
              }}
            />
          </label>
        </Advanced>
      {/if}
    {:else if ed.tool === 'annotate'}
      <p class="text-footnote text-fg-muted">
        {ed.still
          ? 'Pick one to add it, then drag it into place or nudge it with the arrow keys.'
          : 'Adds at the playhead. Drag the block on the timeline to change when it shows.'}
      </p>
      <div class="mt-2 grid grid-cols-3 gap-2">
        {#each [{ kind: 'text', icon: Type, label: 'Title' }, { kind: 'arrow', icon: ArrowUpRight, label: 'Arrow' }, { kind: 'blur', icon: Blur, label: 'Blur' }] as tool (tool.kind)}
          <button
            type="button"
            class="press-card flex flex-col items-center gap-1.5 rounded-[10px] border border-border bg-fill py-3 text-footnote font-medium hover:bg-fill-hover"
            onclick={() => ed.addEffect(tool.kind as 'text' | 'arrow' | 'blur')}
          >
            <Icon icon={tool.icon} size={18} aria-hidden="true" />
            {tool.label}
          </button>
        {/each}
      </div>
      {#if !ed.still}
        <label class="mt-3 flex h-9 items-center justify-between text-callout">
          <span class="flex items-center gap-1"
            >Show keystrokes <InfoTip
              text="Shortcuts you press appear at the bottom. Typing in password fields never shows."
            /></span
          >
          <Switch
            checked={p.keystrokes}
            label="Show keystrokes"
            onchange={(v) => ed.change('Keystrokes', () => (p.keystrokes = v))}
          />
        </label>
      {/if}
      {#if p.effects.length}
        <p class="eyebrow mt-3">{ed.still ? 'On this screenshot' : 'In this video'}</p>
        <ul class="flex flex-col gap-1 py-1">
          {#each p.effects as e (e.id)}
            <li>
              <button
                type="button"
                class={[
                  'ring-inside flex w-full items-center gap-2 rounded-[7px] px-2 py-1.5 text-left text-footnote',
                  ed.selection?.id === e.id ? 'bg-fill-strong' : 'hover:bg-fill',
                ]}
                onclick={() => {
                  ed.selection = { kind: 'effect', id: e.id };
                  ed.seek(e.start + 0.3);
                }}
              >
                <span class="flex-1 truncate">{e.label}</span>
                {#if !ed.still}
                  <span class="text-fg-subtle tabular">{e.start.toFixed(1)}–{e.end.toFixed(1)} s</span>
                {/if}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    {:else if ed.tool === 'camera' && !p.has.camera}
      <p class="text-footnote text-fg-muted">
        This take was recorded without a camera. Turn on Camera in the record dialog to put yourself in a
        bubble next time.
      </p>
    {:else if ed.tool === 'camera'}
      <label class="flex h-10 items-center justify-between text-callout font-medium">
        Show camera
        <Switch
          checked={p.webcam.on}
          label="Show camera"
          onchange={(v) => ed.change('Camera', () => (p.webcam.on = v))}
        />
      </label>
      <p class="eyebrow mt-2">Shape</p>
      <Segmented
        label="Bubble shape"
        size="sm"
        value={p.webcam.shape}
        onchange={(v) => ed.change('Bubble shape', () => (p.webcam.shape = v))}
        options={[
          { value: 'circle', label: 'Circle' },
          { value: 'rounded', label: 'Rounded' },
          { value: 'square', label: 'Square' },
        ]}
      />
      <Slider
        label="Size"
        value={p.webcam.size}
        min={0.1}
        max={0.4}
        step={0.01}
        format={pct}
        onchange={(v) => ed.change('Bubble size', () => (p.webcam.size = v))}
      />
      <p class="eyebrow mt-1">Corner</p>
      <Segmented
        label="Corner"
        size="sm"
        iconsOnly
        value={p.webcam.corner}
        onchange={(v) => ed.change('Bubble corner', () => (p.webcam.corner = v))}
        options={[
          { value: 'tl', label: 'Top left', icon: ArrowUpLeft },
          { value: 'tr', label: 'Top right', icon: ArrowUpRight },
          { value: 'bl', label: 'Bottom left', icon: ArrowDownLeft },
          { value: 'br', label: 'Bottom right', icon: ArrowDownRight },
        ]}
      />
      <label class="mt-1 flex h-9 items-center justify-between text-callout">
        <span class="flex items-center gap-1"
          >Shrink while zoomed <InfoTip
            text="The bubble gets smaller during zooms so it never covers the action."
          /></span
        >
        <Switch
          checked={p.webcam.shrink}
          label="Shrink while zoomed"
          onchange={(v) => ed.change('Shrink', () => (p.webcam.shrink = v))}
        />
      </label>
      <label class="flex h-9 items-center justify-between text-callout">
        <span class="flex items-center gap-1"
          >Mirror <InfoTip text="Flips the camera the way you see yourself in calls." /></span
        >
        <Switch
          checked={p.webcam.mirror}
          label="Mirror"
          onchange={(v) => ed.change('Mirror', () => (p.webcam.mirror = v))}
        />
      </label>
    {:else if ed.tool === 'audio'}
      {#if !p.has.mic && !p.has.system}
        <p class="mb-1 text-footnote text-fg-muted">
          This take was recorded without sound. Turn on Microphone or System audio in the record dialog to
          record it next time.
        </p>
      {/if}
      {#if p.has.mic}<Slider
          label="Microphone"
          value={p.audio.mic}
          min={0}
          max={1.5}
          step={0.05}
          format={pct}
          onchange={(v) => ed.change('Mic volume', () => (p.audio.mic = v))}
        />{/if}
      {#if p.has.system}<Slider
          label="System audio"
          value={p.audio.system}
          min={0}
          max={1.5}
          step={0.05}
          format={pct}
          onchange={(v) => ed.change('System volume', () => (p.audio.system = v))}
        />{/if}
      {#if p.has.mic}<label class="mt-1 flex h-9 items-center justify-between gap-2 text-callout">
          <span class="flex flex-1 items-center gap-1"
            >Clean up my voice <InfoTip
              text="Turns down hum, fan noise and hiss, and evens out your level. Runs on this computer."
            /></span
          >
          {#if ed.voiceBusy === 'cleanup'}<Spinner size={14} label="Cleaning up the voice" />{/if}
          <Switch
            bind:checked={() => p.audio.cleanup, (v) => void ed.setCleanup(v)}
            label="Clean up my voice"
            disabled={ed.voiceBusy !== null}
          />
        </label>
        <label class="flex h-9 items-center justify-between gap-2 text-callout">
          <span class="flex flex-1 items-center gap-1"
            >Cut long silences <InfoTip
              text="Pauses over a second where nothing happens on screen become short ones. Turn it off to bring them back."
            /></span
          >
          {#if ed.voiceBusy === 'silence'}<Spinner size={14} label="Looking for pauses" />{/if}
          <Switch
            bind:checked={() => p.audio.silence, (v) => void ed.setCutSilences(v)}
            label="Cut long silences"
            disabled={ed.voiceBusy !== null}
          />
        </label>{/if}
      <section class="mt-3 rounded-[10px] border border-border p-3" aria-label="Voice-over">
        <h3 class="flex items-center gap-2 text-callout font-semibold">
          <Icon icon={Mic} size={15} aria-hidden="true" /> Voice-over
        </h3>
        {#if ed.voiceOver}
          {@const recording = ed.voiceOver}
          <p class="mt-1.5 flex items-center gap-2 text-callout font-medium tabular" aria-live="polite">
            <StatusDot tone="record" /> Recording · {timecode(
              Math.max(0, ed.outputTimeAt(ed.time) - recording.start),
            )}
          </p>
          <p class="mt-1 text-footnote text-fg-muted">
            The video plays without sound, so nothing echoes into your microphone.
          </p>
          <div class="mt-2.5 flex items-center gap-2">
            <Button size="sm" variant="primary" onclick={() => void ed.finishVoiceOver()}
              >Stop <Keycap chord="space" tone="on-accent" /></Button
            >
            <Button size="sm" variant="ghost" onclick={() => void ed.finishVoiceOver(false)}>Discard</Button>
          </div>
        {:else}
          {#each p.audio.voiceOvers as voice (voice.id)}
            <div class="mt-2.5 flex items-center gap-2.5">
              <span class="min-w-0 flex-1">
                <span class="block truncate text-callout font-semibold">{voice.name}</span>
                <span class="block text-footnote text-fg-muted tabular"
                  >Starts at {timecode(voice.start)} · {timecode(voice.seconds)} long</span
                >
              </span>
              <IconButton
                icon={X}
                label="Remove {voice.name}"
                size="sm"
                onclick={() => ed.removeVoiceOver(voice.id)}
              />
            </div>
            <Slider
              label="Volume"
              value={voice.volume}
              min={0}
              max={1.5}
              step={0.05}
              format={pct}
              onchange={(v) => ed.change('Voice-over volume', () => (voice.volume = v))}
            />
          {:else}
            <p class="mt-1 text-footnote text-fg-muted">
              Talk over the video as it plays from the playhead. It stays in step when you cut or speed things
              up later.
            </p>
          {/each}
          <Button
            class="mt-2.5"
            size="sm"
            icon={Mic}
            disabled={ed.voiceOverBusy}
            onclick={() => void ed.recordVoiceOver(settings.microphoneId)}
            >{ed.voiceOverBusy
              ? 'Starting…'
              : p.audio.voiceOvers.length
                ? 'Record another'
                : 'Record a voice-over'}</Button
          >
        {/if}
      </section>
      <section class="mt-3 rounded-[10px] border border-border p-3" aria-label="Music">
        {#if p.audio.music}
          {@const music = p.audio.music}
          {@const runsOver = music.start + music.seconds > ed.outputDuration + 0.001}
          <div class="flex items-center gap-2.5">
            <span class="grid size-8 shrink-0 place-items-center rounded-[8px] bg-fill text-fg-muted">
              <Icon icon={Music} size={15} aria-hidden="true" />
            </span>
            <span class="min-w-0 flex-1">
              <span class="block truncate text-callout font-semibold" title={music.name}>{music.name}</span>
              <span class="block text-footnote text-fg-muted tabular"
                >{timecode(music.seconds)} · {runsOver ? 'fades out' : 'stops before the end'}</span
              >
            </span>
            <IconButton icon={X} label="Remove music" size="sm" onclick={() => ed.removeMusic()} />
          </div>
          <Slider
            label="Music"
            value={music.volume}
            min={0}
            max={1}
            step={0.05}
            format={pct}
            onchange={(v) => ed.change('Music volume', () => (music.volume = v))}
          />
          <!-- Pulled left so its words line up with the card's other text. -->
          <Button class="-ml-2.5" size="sm" variant="ghost" onclick={() => void ed.addMusic()}
            >Choose other music…</Button
          >
        {:else}
          <h3 class="flex items-center gap-2 text-callout font-semibold">
            <Icon icon={Music} size={15} aria-hidden="true" /> Music
          </h3>
          <p class="mt-1 text-footnote text-fg-muted">
            Plays under the whole video, straight through cuts, and fades out at the end.
          </p>
          <Button class="mt-2.5" size="sm" onclick={() => void ed.addMusic()}>Add music…</Button>
        {/if}
      </section>
    {:else}
      <p class="flex items-center gap-1.5 rounded-[8px] bg-fill px-2.5 py-2 text-footnote text-fg-muted">
        <Icon icon={Lock} size={13} aria-hidden="true" /> Everything here runs on this computer. Nothing is uploaded.
      </p>
      <section class="mt-3 rounded-[10px] border border-border p-3" aria-labelledby="captions-title">
        <div class="flex items-center gap-2">
          <h3 id="captions-title" class="flex flex-1 items-center gap-2 text-callout font-semibold">
            <Icon icon={Captions} size={15} aria-hidden="true" /> Captions
          </h3>
          {#if p.captions.lines.length && !captionJob}
            <Switch
              checked={p.captions.show}
              label="Show captions"
              onchange={(v) => ed.change(v ? 'Show captions' : 'Hide captions', () => (p.captions.show = v))}
            />
          {/if}
        </div>
        {#if captionJob}
          <div class="mt-2.5 flex items-center gap-2" role="status">
            {#if captionJob.stage === 'download' || captionJob.stage === 'transcribe'}
              <ProgressRing value={captionJob.progress} label={jobText} />
            {:else}
              <Spinner size={16} label={jobText} />
            {/if}
            <span class="min-w-0 flex-1 text-footnote text-fg-muted tabular">{jobText}</span>
            <Button size="sm" variant="ghost" onclick={() => ed.cancelCaptions()}>Cancel</Button>
          </div>
          {#if captionJob.stage === 'download'}
            <p class="mt-1.5 text-caption text-fg-subtle">
              Only this once: {modelMb} MB, kept on this computer.
            </p>
          {/if}
        {:else if p.captions.lines.length}
          <p class="mt-1 text-footnote text-fg-muted">
            {p.captions.lines.length}
            {p.captions.lines.length === 1 ? 'line' : 'lines'}{p.captions.language
              ? `, in ${languageName(p.captions.language)}`
              : ''}. Fix any word below; clear a line to remove it.
          </p>
          <div class="mt-2.5">
            <Segmented
              label="Caption style"
              size="sm"
              value={p.captions.style}
              onchange={(v) => ed.change('Caption style', () => (p.captions.style = v))}
              options={[
                { value: 'bar', label: 'Bar' },
                { value: 'bold', label: 'Bold' },
              ]}
            />
          </div>
          <ol
            class="scroll-room -mx-1.5 mt-2 flex max-h-72 flex-col gap-0.5 overflow-y-auto"
            aria-label="Captions"
          >
            {#each p.captions.lines as line (line.id)}
              {@const at = ed.captionStart(line)}
              <li
                class={[
                  'flex items-start gap-1 rounded-[8px] px-1 py-0.5',
                  captionNow === line.id && 'bg-fill',
                ]}
              >
                <button
                  type="button"
                  class="press ring-inside mt-0.5 w-11 shrink-0 rounded-[4px] px-1 py-0.5 text-left text-caption text-fg-subtle tabular hover:text-fg"
                  aria-label="Go to {timecode(at)}"
                  onclick={() => {
                    ed.pause();
                    ed.seek(at);
                  }}>{timecode(at)}</button
                >
                <textarea
                  class="ring-inside min-w-0 flex-1 resize-none rounded-[4px] bg-transparent px-1 py-0.5 text-footnote text-fg field-sizing-content focus:bg-fill"
                  rows={Math.max(1, Math.ceil(width(line.text) / 28))}
                  value={line.text}
                  aria-label="Caption at {timecode(at)}"
                  onfocus={() => {
                    if (!ed.playing) ed.seek(at);
                  }}
                  oninput={(e) => ed.editCaption(line.id, e.currentTarget.value)}
                  onblur={(e) => {
                    if (!e.currentTarget.value.trim()) ed.removeCaption(line.id);
                  }}></textarea>
              </li>
            {/each}
          </ol>
          <div class="mt-2.5 flex flex-wrap items-center gap-1.5">
            <span class="mr-0.5 text-footnote text-fg-muted">Save subtitles</span>
            <Button size="sm" onclick={() => void ed.saveSubtitles('srt')}>SRT…</Button>
            <Button size="sm" onclick={() => void ed.saveSubtitles('vtt')}>WebVTT…</Button>
          </div>
          <div class="mt-1.5 -ml-2.5 flex flex-wrap gap-0.5">
            <Button size="sm" variant="ghost" icon={RefreshCw} onclick={() => void ed.generateCaptions()}
              >Make again</Button
            >
            <Button size="sm" variant="ghost" icon={X} onclick={() => ed.removeCaptions()}>Remove</Button>
          </div>
        {:else if !voice}
          <p class="mt-1 text-footnote text-fg-muted">
            Captions are made from your voice, and this take has none. Record a voice-over to caption it.
          </p>
          <Button class="mt-2.5" size="sm" icon={Mic} onclick={() => (ed.tool = 'audio')}
            >Record a voice-over</Button
          >
        {:else if ed.captionModel?.unsupported}
          <p class="mt-1 text-footnote text-fg-muted">{sentence(ed.captionModel.unsupported)}</p>
        {:else}
          <p class="mt-1 text-footnote text-fg-muted">
            Writes down what you say and times it to the video, in 99 languages.
          </p>
          <div class="mt-2.5 flex flex-col items-start gap-1.5">
            <Button size="sm" icon={Ai} onclick={() => void ed.generateCaptions()}>Generate captions</Button>
            {#if ed.captionModel && !ed.captionModel.ready}
              <span class="text-caption text-fg-subtle"
                >The first time, it downloads a {modelMb} MB speech model.</span
              >
            {/if}
          </div>
        {/if}
      </section>
      {#if !p.has.mic}
        <p class="mt-3 text-footnote text-fg-muted">
          Voice cleanup and cutting silences work on the voice recorded with the take. This one was recorded
          without a microphone.
        </p>
      {:else}
        <section class="mt-2 rounded-[10px] border border-border p-3">
          <h3 class="flex items-center gap-2 text-callout font-semibold">
            <Icon icon={WandSparkles} size={15} aria-hidden="true" /> Voice cleanup
          </h3>
          <p class="mt-1 text-footnote text-fg-muted">
            Turns down hum, fan noise and hiss, and evens out your level.
          </p>
          <div class="mt-2 flex items-center gap-2">
            <Switch
              bind:checked={() => p.audio.cleanup, (v) => void ed.setCleanup(v)}
              label="Voice cleanup"
              hideLabel={false}
              disabled={ed.voiceBusy !== null}
            />
            {#if ed.voiceBusy === 'cleanup'}<Spinner size={14} label="Cleaning up the voice" />{/if}
          </div>
        </section>
        <section class="mt-2 rounded-[10px] border border-border p-3">
          <h3 class="flex items-center gap-2 text-callout font-semibold">
            <Icon icon={Ai} size={15} aria-hidden="true" /> Cut silences
          </h3>
          <p class="mt-1 text-footnote text-fg-muted">
            {#if p.audio.silence}
              Cut {p.audio.silences.length}
              {p.audio.silences.length === 1 ? 'pause' : 'pauses'}, {silenceSeconds.toFixed(1)} s in all. Turn it
              off to bring them back.
            {:else}
              Shortens pauses over a second where nothing happens on screen.
            {/if}
          </p>
          <div class="mt-2 flex items-center gap-2">
            <Switch
              bind:checked={() => p.audio.silence, (v) => void ed.setCutSilences(v)}
              label="Cut silences"
              hideLabel={false}
              disabled={ed.voiceBusy !== null}
            />
            {#if ed.voiceBusy === 'silence'}<Spinner size={14} label="Looking for pauses" />{/if}
          </div>
        </section>
      {/if}
    {/if}

    {#if info.guide}
      <button
        type="button"
        class="press mt-3 self-start rounded-[6px] px-1 py-0.5 text-footnote font-medium text-accent-text hover:underline"
        onclick={() => go('home/learn')}
      >
        Read the guide →
      </button>
    {/if}
  </div>
</aside>
