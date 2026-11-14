<script lang="ts">
  import {
    ArrowDownLeft,
    ArrowDownRight,
    ArrowUpLeft,
    ArrowUpRight,
    BookOpen,
    Captions,
    Droplets,
    Lock,
    RefreshCw,
    Sparkles,
    Type,
    WandSparkles,
  } from '@lucide/svelte';
  import {
    Advanced,
    Button,
    CheckDraw,
    InfoTip,
    ProgressRing,
    Segmented,
    Slider,
    Switch,
    toast,
  } from '@shotmaxxing/ui';
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
    audio: { title: 'Audio', blurb: 'Voice and system sound.' },
    ai: { title: 'AI helpers', blurb: 'Captions, cleanup and cuts, made on this computer.' },
  };

  const info = $derived(TITLES[ed.tool]);
  const pct = (v: number) => `${Math.round(v * 100)}%`;

  let captionProgress = $state(0);
  function generateCaptions() {
    ed.change('Generate captions', () => (p.captions.state = 'busy'));
    captionProgress = 0;
    const started = performance.now();
    const timer = setInterval(() => {
      captionProgress = Math.min(1, (performance.now() - started) / 2600);
      if (captionProgress >= 1) {
        clearInterval(timer);
        ed.change('Captions ready', () => (p.captions.state = 'ready'));
        toast('Captions ready: 6 lines, checked against your audio', { tone: 'success' });
      }
    }, 120);
  }
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
      <BookOpen size={15} aria-hidden="true" />
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
        <Switch
          checked={p.zoom.auto}
          label="Auto zoom"
          onchange={(v) => ed.change('Auto zoom', () => (p.zoom.auto = v))}
        />
      </label>
      <p class="eyebrow mt-1">How far</p>
      <Segmented
        label="Zoom depth"
        size="sm"
        value={p.zoom.depth}
        onchange={(v) => {
          ed.change('Zoom depth', () => (p.zoom.depth = v));
          ed.redoAutoZoom();
        }}
        options={[
          { value: 'subtle', label: 'Subtle' },
          { value: 'standard', label: 'Standard' },
          { value: 'bold', label: 'Bold' },
        ]}
      />
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
      <Button size="sm" icon={RefreshCw} onclick={() => ed.redoAutoZoom()}>Redo auto zoom</Button>
      <p class="mt-1 text-footnote text-fg-muted">Keeps zooms you added or changed by hand.</p>

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
      <Advanced id="zoom">
        <Slider
          label="Hold after a click"
          value={p.zoom.hold}
          min={0.4}
          max={3}
          step={0.1}
          format={(v) => `${v.toFixed(1)} s`}
          onchange={(v) => ed.change('Hold', () => (p.zoom.hold = v))}
        />
        <Slider
          label="Merge moments closer than"
          value={p.zoom.mergeGap}
          min={0}
          max={4}
          step={0.1}
          format={(v) => `${v.toFixed(1)} s`}
          info="Nearby zooms glide into each other instead of zooming out and back in."
          onchange={(v) => ed.change('Merge gap', () => (p.zoom.mergeGap = v))}
        />
        <label class="flex h-9 items-center justify-between text-callout">
          Zoom while typing
          <Switch
            checked={p.zoom.typing}
            label="Zoom while typing"
            onchange={(v) => ed.change('Typing zooms', () => (p.zoom.typing = v))}
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
            onchange={(v) => ed.change('Dwell zooms', () => (p.zoom.dwell = v))}
          />
        </label>
      </Advanced>
    {:else if ed.tool === 'annotate'}
      <p class="text-footnote text-fg-muted">
        Adds at the playhead. Drag the block on the timeline to change when it shows.
      </p>
      <div class="mt-2 grid grid-cols-3 gap-2">
        {#each [{ kind: 'text', icon: Type, label: 'Title' }, { kind: 'arrow', icon: ArrowUpRight, label: 'Arrow' }, { kind: 'blur', icon: Droplets, label: 'Blur' }] as tool (tool.kind)}
          {@const Icon = tool.icon}
          <button
            type="button"
            class="press-card flex flex-col items-center gap-1.5 rounded-[10px] border border-border bg-fill py-3 text-footnote font-medium hover:bg-fill-hover"
            onclick={() => ed.addEffect(tool.kind as 'text' | 'arrow' | 'blur')}
          >
            <Icon size={18} aria-hidden="true" />
            {tool.label}
          </button>
        {/each}
      </div>
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
      {#if p.effects.length}
        <p class="eyebrow mt-3">In this video</p>
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
                <span class="text-fg-subtle tabular">{e.start.toFixed(1)}–{e.end.toFixed(1)} s</span>
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
    {:else if ed.tool === 'audio' && !p.has.mic && !p.has.system}
      <p class="text-footnote text-fg-muted">
        This take was recorded without sound. Turn on Microphone or System audio in the record dialog to
        record it next time.
      </p>
    {:else if ed.tool === 'audio'}
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
      {#if p.has.mic}<label class="mt-1 flex h-9 items-center justify-between text-callout">
          <span class="flex items-center gap-1"
            >Clean up my voice <InfoTip
              text="Removes hum, fans and room echo. Runs on this computer."
            /></span
          >
          <Switch
            checked={p.audio.cleanup}
            label="Clean up my voice"
            onchange={(v) => ed.change('Voice cleanup', () => (p.audio.cleanup = v))}
          />
        </label>
        <label class="flex h-9 items-center justify-between text-callout">
          <span class="flex items-center gap-1"
            >Cut long silences <InfoTip
              text="Pauses longer than a second become short ones. You can bring any back on the timeline."
            /></span
          >
          <Switch
            checked={p.audio.silence}
            label="Cut long silences"
            onchange={(v) => ed.change('Cut silences', () => (p.audio.silence = v))}
          />
        </label>{/if}
    {:else if !p.has.mic}
      <p class="flex items-center gap-1.5 rounded-[8px] bg-fill px-2.5 py-2 text-footnote text-fg-muted">
        <Lock size={13} aria-hidden="true" /> Everything here runs on this computer. Nothing is uploaded.
      </p>
      <p class="mt-3 text-footnote text-fg-muted">
        Captions, voice cleanup and cutting silences work on your voice. This take was recorded without a
        microphone.
      </p>
    {:else}
      <p class="flex items-center gap-1.5 rounded-[8px] bg-fill px-2.5 py-2 text-footnote text-fg-muted">
        <Lock size={13} aria-hidden="true" /> Everything here runs on this computer. Nothing is uploaded.
      </p>
      <section class="mt-3 rounded-[10px] border border-border p-3">
        <h3 class="flex items-center gap-2 text-callout font-semibold">
          <Captions size={15} aria-hidden="true" /> Captions
        </h3>
        <p class="mt-1 text-footnote text-fg-muted">
          Transcribes your voice with Whisper, word by word, in 99 languages.
        </p>
        <div class="mt-2.5 flex items-center gap-2">
          {#if p.captions.state === 'ready'}
            <span class="flex items-center gap-1.5 text-footnote font-semibold text-success"
              ><CheckDraw size={14} /> 6 lines</span
            >
            <span class="flex-1"></span>
            <Switch
              checked={p.captions.show}
              label="Show captions"
              onchange={(v) => ed.change('Show captions', () => (p.captions.show = v))}
            />
          {:else if p.captions.state === 'busy'}
            <ProgressRing value={captionProgress} label="Transcribing" />
            <span class="text-footnote text-fg-muted tabular"
              >Transcribing… {Math.round(captionProgress * 100)}%</span
            >
          {:else}
            <div class="flex flex-col items-start gap-1.5">
              <Button size="sm" icon={Sparkles} onclick={generateCaptions}>Generate captions</Button>
              <span class="text-caption text-fg-subtle">The first time downloads a 140 MB model.</span>
            </div>
          {/if}
        </div>
        {#if p.captions.state === 'ready'}
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
        {/if}
      </section>
      <section class="mt-2 rounded-[10px] border border-border p-3">
        <h3 class="flex items-center gap-2 text-callout font-semibold">
          <WandSparkles size={15} aria-hidden="true" /> Voice cleanup
        </h3>
        <p class="mt-1 text-footnote text-fg-muted">Removes background noise and evens out your level.</p>
        <div class="mt-2">
          <Switch
            checked={p.audio.cleanup}
            label="Voice cleanup"
            hideLabel={false}
            onchange={(v) => ed.change('Voice cleanup', () => (p.audio.cleanup = v))}
          />
        </div>
      </section>
      <section class="mt-2 rounded-[10px] border border-border p-3">
        <h3 class="flex items-center gap-2 text-callout font-semibold">
          <Sparkles size={15} aria-hidden="true" /> Cut silences
        </h3>
        <p class="mt-1 text-footnote text-fg-muted">
          Shortens long pauses. Found 2 in this take, 3.4 s in all.
        </p>
        <div class="mt-2">
          <Switch
            checked={p.audio.silence}
            label="Cut silences"
            hideLabel={false}
            onchange={(v) => ed.change('Cut silences', () => (p.audio.silence = v))}
          />
        </div>
      </section>
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
