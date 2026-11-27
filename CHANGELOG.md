# Changelog

What changed in each release. Versions follow [semantic versioning](https://semver.org); until 1.0, a minor version
can change how things work.

## 0.3.3 (2026-11-27)

A fix for Windows, where recordings played back at about 4 frames a second.

- **Smooth recordings on Windows:** the picture in the editor and in exports moved at about 4 frames a second while
  the cursor and clicks stayed smooth, because each frame waited on the encoder, and the encoder converted it on the
  CPU first. Frames are now converted on the graphics card and handed to the encoder without holding capture up, so a
  recording keeps the screen's pace, up to the frame rate you chose. A computer without a hardware video encoder
  records at about 1080p so it stays smooth. If the computer is too busy to keep up, the newest picture is kept and
  the editor says how many frames were dropped.
- **The log says more about recordings:** which encoder Windows used, the size recorded, and each recording's frame
  rate, to send along if a recording still stutters.

## 0.3.2 (2026-11-18)

A fix for Windows, where Shotmaxxing closed as soon as a recording started.

- **Recording works on Windows:** with **System audio** on (as it is unless you turn it off), starting a recording
  ended the app straight away on Windows 10 (version 2004) and later. Setting up the recording of the computer's sound
  freed the same memory twice. It's freed once now, and the computer's sound is recorded, still leaving out
  Shotmaxxing's own sounds. Found and checked on a Windows machine that records a window, its sound and clicks the
  way the app does.
- **A log file:** Shotmaxxing now notes what it does as it runs, including any crash and where it happened, in
  `%LOCALAPPDATA%\Shotmaxxing\logs` on Windows and `~/Library/Logs/Shotmaxxing` on a Mac. **Settings → About → Show
  log file** opens it, to send along when you report a problem.

## 0.3.1 (2026-11-17)

A fix for Windows, where the New screenshot and New recording windows froze, and Shotmaxxing now updates itself.

- **Windows no longer freezes on a new window:** opening New screenshot or New recording (and the editor and the
  recording pill) from inside the app stayed white and took Shotmaxxing to Not Responding, because
  Windows' web view can't build a window from where the app asked for it. Windows are now built from the right place on
  every system. Install 0.3.1 over 0.3.0 with the same installer or install line; your projects and settings stay.
- **Shotmaxxing updates itself:** it looks for a new version at launch and every six hours and downloads it quietly.
  **Update to …** appears at the bottom of the sidebar and opens what changed in every version since yours, with
  **Restart to update** (which waits while you're recording or exporting), **Later** and **Skip this version**.
  Downloads are checked against Shotmaxxing's own signing key before they're installed. **Settings → About** shows the
  version and has **Check for updates**, and the first launch of a new version shows what's new.
- **Agents can style everything the editor can:** `style_project` now sets a built-in photo, a picture file, a solid
  colour or a gradient behind the window, the background blur, and the cursor's style, visibility, click effect, click
  bounce, bounce speed, sway and motion blur.
- **Drop a picture on the editor** while the Background panel is open and it becomes the background.
- **Recent colours and gradients:** the ones you make come back under Recent in the Color and Gradient tabs.
- **Photos you can share:** the seven bundled pictures that came from Apple and Oppo wallpapers are gone, replaced by
  three original ones (Lime haze, Ocean mist and Plum dusk), so there are 20.
- **A fix for starting up:** the app no longer mistakes a socket that's closing for another Shotmaxxing already
  running.
- **New screenshots** of the 0.3 editor in the README, the guides and the press kit, with one of the Cursor tool.

## 0.3.0 (2026-11-17)

Cursors and backgrounds, made to look the way you want: eight cursor styles with sway and a click bounce, and photos,
colours, gradients and a blur behind the recording, all exported exactly as the editor shows them. Agents get along
better too.

- **Eight cursors, picked from tiles:** macOS, macOS white, Windows, Outline, Dot, Ring, Bold and Hand. The editor and
  the export draw them from one shared shape file, so what you pick is what you get. A **Show cursor** switch hides
  it from the whole video.
- **Cursor sliders:** Size, Smoothing, **Motion blur** (now an amount rather than on or off), **Sway** (the cursor
  leans into fast sideways moves), and with the Press click effect, **Click bounce** and **Bounce speed**. Sliders are
  new too: one bar that fills to the value, with the label and value inside it; click the value to type one.
- **Backgrounds as Image · Color · Gradient:** 24 photos and abstract wallpapers that ship with the app, **Upload
  custom** for your own PNG, JPEG or WebP (copied into the project), any solid colour from a picker, a hex code or
  swatches, the gradient wallpapers, and **Make your own** gradient from two colours and an angle. Picking a tab puts
  its background on at once, and it remembers what you chose there last.
- **Background blur:** softens whatever is behind the recording, up to 40 px at 1080p, the same Gaussian in the
  editor and the export. It sits right under the tabs.
- **Choosing Windows on a Mac exports the Windows cursor:** projects used to save that choice as "native", which drew
  the Mac arrow.
- **Panel controls keep their height:** in a long panel, the tabs, buttons and sliders were squeezed thinner than they
  should be, and sliders could vanish below the photos.
- **Agents take turns:** a take requested while another agent's is on the card now waits up to two minutes for its
  turn instead of failing with "busy". A request the app hangs up on at once, because it was starting or quitting, is
  asked once more.
- **`plan_zooms` works in Claude Code:** its reply was a bare list, which Claude Code rejects as malformed. Every tool
  now replies with a JSON object.
- **Website:** a new hero, "Beautiful screen recordings, by you or your AI.", with a video for recording yourself and
  one of Claude Code filming a demo; comparison pages (Screen Studio, Recordly, Loom, Cap), use-case pages and a press
  kit; share images, a sitemap, llms.txt and structured data for search and answer engines.

## 0.2.1 (2026-11-16)

Permissions that stay: Shotmaxxing sees each one as it is, and from this release on macOS keeps them when Shotmaxxing
updates. After updating to 0.2.1, turn Screen Recording and Accessibility on once more (the first entry says how).

- **Permissions carry over between updates:** the macOS app in every release is now signed with one fixed identity.
  It's self-signed, so it costs nothing and needs no Apple account. Before, each build was signed ad-hoc and looked like a
  new app to macOS, so after an update the switches in System Settings stayed on but no longer counted. 0.2.1 is the
  first build with the identity, so macOS sees it as new one last time: in System Settings › Privacy & Security,
  select Shotmaxxing under Screen & System Audio Recording (Screen Recording on macOS 14) and under Accessibility,
  remove it with the − button, then choose Allow in Shotmaxxing.
- **Install in one line:** `curl -fsSL https://raw.githubusercontent.com/subh05sus/shotmaxxing/main/install.sh | bash`
  on a Mac, and `irm https://raw.githubusercontent.com/subh05sus/shotmaxxing/main/install.ps1 | iex` in PowerShell on
  Windows. Each downloads the release, checks its SHA-256, installs it and opens it, without the "Not Opened" dialog
  on the Mac or the SmartScreen warning on Windows. The same line updates it. The [README](README.md#install) leads
  with them.
- **Homebrew:** the app the cask installs opens on the first click too, and the cask no longer makes Homebrew warn
  about its `depends_on macos:` line.
- **Setup sees permissions as they are:** Screen Recording turned on while Shotmaxxing is open shows as on, with a
  Reopen button (macOS gives it to an open app only after a reopen), and reopening comes back to the same step of
  setup. A switch that stays off after a trip to System Settings gets a note on what else to try, Allow always opens
  System Settings for Accessibility, and setup can go on without Screen Recording: the practice take then shows how
  to turn it on. The record dialog offers Reopen too, and its Screen Recording message says to reopen Shotmaxxing
  rather than to try again.
- **A notice when macOS forgets:** when Screen Recording worked at an earlier launch and doesn't now, Shotmaxxing says
  so once at launch, and Fix opens setup's Permissions step. The first launch after updating from 0.2.0 or earlier,
  which didn't note whether it worked, says once that Screen Recording is off for this version, when it is.

## 0.2.0 (2026-11-16)

The second preview: agents plan their takes from your app's code, a red switch lets them film without asking, the
first launch is new, and Home gets one search.

- **The logo builds itself on the first launch:** a point of lime light draws the ring, the blades swing in and
  click shut, light fills the icon, and the view flies through the lens onto the welcome. Any key or click skips it;
  Reduce Motion gets a fade, and Low Power Mode goes straight to the welcome. Every later start plays a short version.
- **Setup is a set of full-window scenes:** each step has big type and one action, the view pans from one step to the
  next, and dots replace the stepper. The welcome plays a 10-second show of a sign-up page recording itself (a click,
  an auto zoom that says why, the take saved). Each permission acts out what it allows, and agents that aren't
  installed share one line instead of a row each. Finishing closes the shutter and opens it on the Library.
- **Content packs:** add wallpapers and looks from a JSON file in Settings → Content packs. They show in the editor's
  Background tool, a look applies its settings in one click, and projects keep a copy of the wallpaper they use.
  `shotmaxxing pack check` and `pack list` check and list packs, and agents can choose pack wallpapers.
  [How to make one](docs/content-packs.md).
- **One search in Home:** the top bar's field filters the Library as you type and lists the matching recordings and
  actions under it, so Enter opens the first match or runs the highlighted action. ⌘K jumps into it; the Library's
  own filter is gone, and the editor keeps ⌘K as its palette.
- **Record and Screenshot float:** they sit in a pill at the bottom of the Home window, with their shortcuts, instead
  of in the top bar.
- **Agents plan from the code:** an agent working in your app's code learns the flow there, writes a plan in plain
  steps for you to check, tests each step against the screen, then films it. The take card shows the plan. The new
  `check_take` tool and `shotmaxxing take check` look up every target of a take script on screen and suggest what to
  try instead of any that would stop it, and the `plan-demo` prompt (`/mcp__shotmaxxing__plan-demo` in Claude Code)
  walks an agent through the whole routine.
- **Allow everything:** a red switch in Settings › Agents and on the Agents page lets agents film without the take
  card asking, until you turn it off or quit. It covers requests that come through the `shotmaxxing` command, as
  agents' do; stopping a take, password fields and the take card work as before. The menu-bar icon and the take card
  show a red dot while it's on.
- **A red edge** goes round every screen while an agent's take moves the pointer and types. It lets clicks through and
  stays out of the recording.
- **The installers have their own pictures:** the disk image window shows dragging Shotmaxxing to Applications and
  how to open it the first time, and the Windows setup and `.msi` pages are drawn in the app's colours.
- **Homebrew:** `brew tap subh05sus/shotmaxxing https://github.com/subh05sus/shotmaxxing`, then
  `brew install --cask subh05sus/shotmaxxing/shotmaxxing`. winget manifests are ready to submit.
- **Fixed:** with the Light theme on a Mac in Dark Mode, the sidebar stayed dark and its labels were unreadable. The
  window material, menus and dialogs now follow the theme chosen in Settings, from the first frame of the next launch.
- **Fixed:** an app with two full-screen windows (Chrome on two Spaces, say) showed only one of them in the picker,
  and picking a window on another Space could fail with "no picture came back". Every full-screen window is listed.
  Picking one asks macOS for that window rather than its app's front window and, when Shotmaxxing may use
  Accessibility, raises it above the app's other windows, asking again if the Mac lands on the app's other Space. A
  picture that still fails says why.
- **Fixed:** the editor tour's outline sometimes didn't show: it was measured before the editor had laid out, and
  against the window's edge it was cut off. It now finds its target as it appears, follows it, stays inside the
  window, and uses the focus colour, so it reads in the Light theme too.

## 0.1.0 (2026-11-16)

The first preview. It's built for macOS. The Windows version is written but hasn't been tested on a Windows computer
yet.

### Record

- Record a display, a window or an area, with the computer's sound, a microphone and a camera. Recordings are HEVC in
  a fragmented MP4, encoded by the Mac's media engine, so a crash loses at most two seconds.
- Window recordings keep their menus. The window's app is recorded on its display and cropped to the window, so a
  dropdown or sheet over the window shows at full size, and the crop follows the window as it moves. Menus that
  another process draws over the window, such as System Settings' panes, are added as they appear, never a password
  or sign-in sheet.
- Full-screen apps are listed from their own Spaces and recorded with their toolbars. With Stage Manager on, a parked
  window comes back to the stage first, at its own size.
- Screenshots: ⌥⇧4 picks a screen or a window for a full-resolution picture (without the pointer), opens it in the
  editor to frame and mark up, then copies it or saves a PNG. The next screenshot starts with the last one's look.
- ⌥⇧5 starts and stops a recording. While you record, a small pill shows the time, with Stop and a menu to restart
  or discard the recording; the menu-bar icon can stop it too.
- Shotmaxxing's own windows never show in a recording, and clicks on them, the pill or the menu-bar icon aren't
  recorded.

### Edit

- Auto zoom follows clicks, typing, the control in use and what changes on screen, and says why each zoom is there.
  Choose how far it zooms and whether it glides or cuts, or add, move and remove zooms by hand.
- A rendered cursor: size, smoothing, styles, click effects, snapping to clicks, a spotlight, motion blur and click
  sounds.
- The look: wallpapers, padding, corners, a shadow, and 16:9, 4:3, 1:1 or 9:16.
- A timeline to trim, split and change speed, with text, arrows and blur.
- Sound: voice cleanup and silence cutting on the computer, a voice-over recorded in the editor, and music under the
  whole video that fades out at the end.
- Captions transcribed on the computer with Whisper (a one-time 60 MB model download, checked against a pinned hash).
  Every word can be fixed, they come in two styles, they're drawn into exports exactly as previewed, and they save
  as SRT or WebVTT.
- A camera bubble, autosave and version history.
- Export to MP4 (H.264 or HEVC) or GIF with the sound mixed, or any frame as a PNG. Exports wait in a queue and render
  one at a time.

### Agents

- Connect Claude Code, Claude Desktop, Codex, Cursor, VS Code, Windsurf, OpenCode or Gemini CLI from the Agents page,
  in one click. Disconnect takes the entry out again.
- `shotmaxxing mcp`, an MCP server with 15 tools: list sources and devices, record, tidy up, style and export, and
  plan and perform takes.
- Planned takes: the agent writes a take script, and Shotmaxxing performs it at a human pace while it records, with
  curved pointer moves, settled clicks and readable typing. Controls are found through Accessibility and checked
  again just before each click.
- The person allows each agent session and each take on the take card, which names the program that's asking and
  who signed it. The card follows the take live, with Pause, Take over and Stop.
- Takes never type into password fields. Pressing Esc three times or moving the mouse stops one at once. A sign-in
  step hands control to the person until they hand it back (⌥⌘Return), and that wait is cut from the video.
- The `shotmaxxing` command: sources, devices, record, export, info, zoom, take validate, take schema, take prepare,
  take run and open, with `--json`.
- An agent skill that teaches agents the workflow ([skills/shotmaxxing](skills/shotmaxxing/SKILL.md)).

### Stays cool

- Nothing animates or redraws while the app sits idle, and there are no live previews while recording.
- Low power mode turns the work down on battery or when the computer runs warm. The recording itself is never slowed
  down.

### Privacy

- Recordings, projects and captions stay on the computer. There's no telemetry, analytics or account.
- The app goes online for two things: once a day it asks GitHub whether a newer release is out (Settings → Check for
  updates turns that off), and the captions' model downloads once, the first time you make captions.

### Windows

Recording (the screen, windows, the pointer, clicks and shortcuts, the computer's sound, microphones and the camera),
screenshots, export, captions, voice-overs, agent takes (through UI Automation and SendInput) and the control pipe are
all written, build on CI and pass its tests, but haven't been tried on a Windows computer. Expect rough edges.

### Known limitations

- The macOS app isn't notarized and the Windows installer isn't signed yet, so each system warns the first time. The
  [README](README.md#install) says how to open them.
- In a window recording, a menu that reaches past the window's edge is cut at that edge. Record the screen or an area
  to keep it whole.
- Takes use the real pointer and keyboard, so leave the computer alone while one runs. Takes on a virtual display of
  their own are planned.
- There are no automatic updates yet: the app tells you when a new version is out and links to it.
