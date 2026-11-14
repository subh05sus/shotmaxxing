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
- **Pixels stay native.** Video frames are drawn by the Rust compositor, never by the web UI layer.

## Motion: the twelve principles, applied

| Principle | How we use it |
| --- | --- |
| Squash and stretch | A pressed button scales to 0.97, and a clicked cursor squashes briefly. Small amounts only. |
| Anticipation | Big moves wind up first. The Record button dips before the countdown, and a panel nudges back 2 px before it slides open. |
| Staging | One focal point per moment. When a sheet opens, everything behind it dims. A coach mark spotlights one control. |
| Pose to pose | We define start and end states and let the browser interpolate with springs and CSS transitions, with no hand-built frame loops. |
| Follow through | A settling panel overshoots by a few pixels and comes to rest. Zoom blocks land on the timeline and settle. |
| Slow in and slow out | Nothing moves at constant speed. Use springs or `cubic-bezier(0.2, 0, 0, 1)`, never `linear` (except progress bars). |
| Arcs | Things that travel far, such as a clip flying to the export queue or the cursor in agent takes, follow a curve, not a straight line. |
| Secondary action | A small flourish supports the main one: the check draws after Saved, and the export count rolls as the bar fills. |
| Timing | Every UI animation finishes within **300 ms**. Exits are about two thirds the length of entrances. |
| Exaggeration | Moments that matter land clearly: the first export gets confetti, and a successful agent take gets a short shine. |
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

## House rules

- Never put a coloured or thick edge down one side of a card, row, chip or callout. Show status with a small dot, an
  icon with a word, or a tinted fill.
- Never let an outline, ring or glow be cut off at a margin. Scroll areas get inner padding, and focus rings on rows
  are drawn inside.
- Colours come from tokens only. Both themes are designed in their own right; dark is never an inverted light theme.
- Every state is designed: loading, empty, error, slow, offline, partial and sample data.
- Words are plain: sentence case, verbs on buttons, numbers with units.
