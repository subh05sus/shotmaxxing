---
name: shotmaxxing
description: Film polished demo videos of apps on this computer with Shotmaxxing. Record a screen or a window, tidy the take (cut silences, clean up the voice, replan zooms, style the canvas) and export an MP4 or a GIF. Use it when someone asks to record, film or make a demo, walkthrough, tutorial, product video or a GIF of something on screen, and the Shotmaxxing MCP tools (start_recording, export_project and the rest) are available.
---

# Filming demos with Shotmaxxing

Shotmaxxing records the screen into an editable project (a `.smx` folder) and exports a finished video:

- a wallpaper behind the window;
- smooth automatic zooms on clicks and typing;
- a large, clean cursor;
- the voice and camera, when recorded.

The person can open any project in the Shotmaxxing editor afterwards and change anything.

## Planned takes (the best results)

When you can see the app's controls, plan the whole performance and let Shotmaxxing perform it:

1. Call `prepare_take` with the app. Read the controls it lists and look at the picture of the window.
2. Write a take script. Name controls by their ids (`click: button.export`). Use `{ find: "Label" }` for controls
   that only appear later, and `wait: { for: idle }` after anything that loads.
3. Add `say:` beats for short captions. Use a `user_step:` beat for anything only the person may do, such as
   signing in.
4. Call `perform_take` with `"dry_run": true`, check the timeline, then call it again for real. Use
   `output: { mode: video }` to get a file straight away. When the Shotmaxxing app is open, the person allows the
   take there first, so the call can wait a minute or two for them. If they say no, don't try again: ask them what
   they'd like instead.
5. If the reply says the take stopped:
   - a control was gone or disabled, or Esc was pressed: tell the person, fix the script, and ask before trying
     again;
   - the person moved the mouse or pressed Stop: they took over, so leave the rest to them.

## Filming by hand

1. **Plan the take before recording.** Write down the steps, the window to film and roughly how long it should
   run. Short is better: 30 to 90 seconds for a feature, under 15 seconds for a GIF.
2. **Stage the app.** Get it to the starting page, sign in and dismiss pop-ups before you record. Close or hide
   anything private: other windows, notifications, personal data. Never film passwords or keys being typed. Hand
   those steps to the person.
3. **Find the source.** Call `list_sources` and pick the window by its app and title. Filming a window looks better
   than the whole screen. Use `app` in `start_recording` when one window of that app is open.
4. **Record.** Call `start_recording`:
   - add `"microphone": "default"` only when the person will narrate;
   - add `"system_audio": true` only when the app's sound matters;
   - set `max_seconds` a little above the planned length.
5. **Perform.** Move with purpose:
   - pause half a second before each click, so the zoom has something to land on;
   - type at a human pace, and let each result show for a beat before moving on.
   
   If the person performs the demo, tell them when recording has started, and wait for them to say they're done.
6. **Stop.** Call `stop_recording` and read the reply:
   - `warnings`, such as a microphone that wasn't allowed: pass them on in plain words;
   - `dropped_frames`;
   - the planned `zooms`.
7. **Tidy.** Each step is optional:
   - `cut_silences`, when there's narration with long pauses;
   - `clean_voice`, when there's a microphone;
   - `plan_zooms` with `subtle` or `bold`, when the zooms feel too strong or too weak;
   - `style_project`, for the wallpaper, shape (`16:9`, `1:1`, `9:16` for phones), padding and corners.
8. **Export.** Call `export_project`:
   - the default MP4 at 1080p suits most uses;
   - `"format": "gif"` gives a 480p, 15 fps loop for docs and chats;
   - `"format": "png"` with `"at"` (seconds into the recording, as zoom times are given) saves one styled frame,
     for a screenshot in docs or a post;
   - give `output` when the person wants the file somewhere specific.
   
   Report the file's path and length.

## Good to know

- **Permissions.** macOS asks for Screen Recording the first time. The permission belongs to the app that started
  the MCP server: the terminal for command-line agents, otherwise the agent's own app. If recording fails for
  this reason, ask the person to allow it in System Settings → Privacy & Security, then restart that app.
- **One take at a time.** If `start_recording` says a take is already recording, call `recording_status`, and
  stop that take first if it's yours.
- **Undo.** `plan_zooms` and `cut_silences` keep the previous edits in the project's history. Everything can also
  be changed by hand in the editor.
- **Checking scripts.** `validate_take` lists a script's problems. `perform_take` with `dry_run` also shows when
  each beat would happen.
- **Stopping.** The person can stop any take by pressing Esc three times or moving the mouse. Takes never type into
  password fields.
- **Where things go.** Projects go to `~/Movies/Shotmaxxing` unless you pass `folder`. Exports go next to the project
  unless you pass `output`.
