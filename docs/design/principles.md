# Design principles

Shotmaxxing should feel alive, fast and obvious, and it should never make a laptop run hot. These principles
apply to every screen, component and animation. A change that breaks one needs a written reason in its pull request.

## Performance comes first

- **No idle work.** When nothing changes on screen, nothing animates, redraws or polls. Zero frames while idle.
- **Cheap motion only.** Animate `transform` and `opacity`, and nothing else. Never animate layout, size, blur
  or shadows.
- **Native materials, not CSS blur.** Translucency comes from the OS (macOS vibrancy and Liquid Glass, Windows Mica
  and Acrylic), and only on chrome and floating controls.
- **Low power and Reduce Motion drop everything to fades.** Low power mode turns on automatically when the machine
  is hot or on battery.
- **Pixels stay out of the UI's code.** Video frames never travel through JavaScript or the bridge: the editor
  shows the recording with the webview's own hardware-decoded video player, and the export draws every frame in the
  Rust compositor.

## Motion: the twelve principles, applied

| Principle | How we use it |
| --- | --- |
| Squash and stretch | A pressed button scales to 0.97, and a clicked cursor squashes briefly. Small amounts only. |
| Anticipation | Big moments get a beat of warning: the record countdown says 3, 2, 1 before filming starts, and a pressed tab squishes before it springs back. |
| Staging | One focal point per moment. When a sheet opens, everything behind it dims. A coach mark spotlights one control. |
| Pose to pose | We define start and end states and let the browser interpolate with springs and CSS transitions, with no hand-built frame loops. |
| Follow through | The label or icon just picked pops a little past full size, then settles back. |
| Slow in and slow out | Nothing moves at constant speed. Use springs or `cubic-bezier(0.2, 0, 0, 1)`, never `linear` (except progress bars). |
| Arcs | Things that travel far follow a curve, not a straight line: the pointer in an agent's take moves in gentle arcs. |
| Secondary action | A small flourish supports the main one: the check beside Saved draws itself. |
| Timing | Every UI animation finishes within **300 ms**. Exits are about two thirds the length of entrances. One exception: a tab's sliding thumb lands in about 200 ms and then settles a few pixels by 360 ms. |
| Exaggeration | Moments that matter land clearly: the first export gets confetti, once. |
| Solid drawing | Flat surfaces get depth through layered shadows and light edges, so floating things clearly sit above content. |
| Appeal | The sum of the details: aligned edges, consistent radii, considered words, every state designed. |

## Interaction laws

| Law | Rule in Shotmaxxing |
| --- | --- |
| **Fitts's law** | Frequent targets are big and close: Record, Play and Export are the largest controls, and the timeline playhead handle is wide. Destructive actions sit far from frequent ones. |
| **Bigger hit areas** | Every interactive element has at least a 28 × 28 px hit area (36 px for primary actions), even when the visible icon is 16 px. Timeline clip edges have 8 px grab zones. |
| **Hick's law** | Show few choices at once. Presets come first (Subtle, Standard, Bold), with Custom and Advanced behind a click. One primary action per screen. |
| **Miller's law** | Chunk into groups of about seven or fewer: rail items, panel sections, onboarding steps and coach-mark stops. |
| **Doherty threshold** | Respond within 400 ms, and aim for 100 ms. Show the state change instantly (optimistic), then confirm. Anything slower shows progress at once. |
| **Postel's law** | Accept messy input: times like `1:05`, `65s` or `1m5s`, pasted colours in any format, any video or image file. Return clean, normalised output. |
| **Jakob's law** | Work like the tools people know: Space plays, J/K/L shuttle, ⌘Z undoes, the timeline behaves like Final Cut, Premiere and Screen Studio, and shortcuts follow each OS's conventions. |
| **Frequency of use** | The more often something is used, the less it animates. Scrubbing, play/pause, timeline drags and typing get instant feedback with no decorative motion. Rare moments, like onboarding or the first export, get the most motion. |

## Craft details

**Type**
- **X-height and family.** Use the system UI font on each OS (SF Pro on macOS, Segoe UI Variable on Windows). Both
  have a large x-height that stays readable at 13 px.
- **Tracking.** Large titles get slightly tighter tracking (−0.01 to −0.02 em), small caps labels a little looser
  (+0.04 em), and body text is left as is.
- **Kerning.** Keep the font's kerning on (`font-kerning: normal`). Hand-kern only the wordmark and large display
  numbers.
- **Leading.** Body text at 1.45, UI labels at 1.2, large titles at 1.1.
- **Line length.** Guides and long help text are capped at about 65 characters (`max-width: 65ch`).
- **Widows.** Headings and short paragraphs use `text-wrap: balance` or `pretty`, so no single word sits alone on
  the last line.
- **Tabular numbers.** Timecodes, durations, file sizes, percentages and anything that updates live use
  `font-variant-numeric: tabular-nums`, so digits never jiggle.

**Layout**
- **Optical centring.** Centre things by eye: play triangles shift right by about 8% of their width, and icons with
  heavy sides are nudged so they look centred.
- **No layout shift.** Thumbnails, panels and lists reserve their final size before content loads (fixed aspect
  ratios and skeletons of the same size). Status text like "Saved" swaps in place without moving anything.
- **Safe area.** Keep content out of window corners and away from the macOS traffic lights and Windows caption
  buttons. Floating cards keep at least 12 px from window edges.

**Shape**
- **Capsules for choices.** Tabs and segmented controls are capsules on both systems: the track and its sliding
  thumb have fully round ends.

**Icons**
- **One set, one weight.** Icons come from the free [Pikaicons](https://pikaicons.com/free-icons) in their Stroke
  style. Where Pika has no icon for an idea, the free [Hugeicons](https://hugeicons.com) stroke-rounded set fills in.
  AI has one mark everywhere, Hugeicons' AI Magic, and nothing else uses it. Both sets are MIT licensed, drawn on a
  24 px grid and coloured with `currentColor`, and `Icon` draws both at a stroke of 1.75 so they read as one set.
  Below 16 px it thickens the stroke just enough to stay 1.15 px on screen, so small icons don't turn faint at 100%
  scaling on Windows.
- **One map.** Screens import icons from `@shotmaxxing/ui/icons` by what they mean (`Mic`, `Stop`, `Blur`), never
  from the icon packages, so changing a drawing is one line in `packages/ui/src/lib/icons.ts`.
- **Solid marks.** Record is a solid dot (`RecordDot`). Stop, Play and Pause are filled (`fill="currentColor"`) when
  they're a transport or the main action.

**Colour**
- **OKLCH.** All colour tokens are defined in OKLCH, so hover, pressed and disabled steps change lightness evenly in
  both themes, and accent tints stay the same perceived brightness.

**Interaction**
- **Touch target.** The whole row, card or chip is clickable, not just its icon or text.
- **Affordance.** Things that can be dragged show a grip on hover and a grab cursor. Clip edges show resize
  handles. Editable values look like fields, not labels.
- **Optimistic updates.** Toggles, renames, deletes and reorders show the result at once and reconcile with the
  core afterwards, rolling back with a clear message if something fails.
- **Combobox.** Fonts, colours, presets and folder pickers accept typing and also offer a filtered list.
- **Progressive disclosure.** Each panel shows presets first. Advanced controls sit behind "Advanced" and stay
  open once opened.

**Motion**
- **Origin-aware animation.** Menus, popovers and sheets grow from the control that opened them.
- **Stagger.** Groups enter one after another, 30–40 ms apart, capped at 8 items so a list never drags.
- **Tabs.** Switching tabs slides the thumb on a spring that overshoots by about 6% and settles; the label it lands
  on pops slightly, and a pressed tab squishes to 0.96. Only transforms move, and Reduce Motion leaves a fade.
- **Interruptible animation.** Every animation can be reversed mid-way. Springs retarget from their current
  position, and nothing blocks input while it moves.
- **Shared element transitions.** A Library thumbnail grows into the editor canvas when a project opens. The
  Export button becomes the progress indicator.

## House rules

- Never put a coloured or thick edge down one side of a card, row, chip or callout. Show status with a small dot, an
  icon with a word, or a tinted fill.
- Never let an outline, ring or glow be cut off at a margin. Scroll areas get inner padding, and focus rings on rows
  are drawn inside.
- Colours come from tokens only. Both themes are designed in their own right; dark is never an inverted light theme.
- Every state is designed: loading, empty, error, slow, offline, partial and sample data.
- Words are plain: sentence case, verbs on buttons, numbers with units.
