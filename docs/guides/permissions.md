# Permissions

On a Mac, Shotmaxxing needs one permission to record and asks for the others only when you use what they're for.
Setup shows each switch live, so you see the moment one turns on.

| Permission | What it's for | Needed? |
| --- | --- | --- |
| Screen Recording | Filming your screen. Without it, recordings come out blank | Yes |
| Microphone | Recording your voice while you show something | When you turn the microphone on |
| Camera | Putting you in a bubble next to the action | When you turn the camera on |
| Accessibility | Noticing clicks and typing for auto zoom, and letting agents press buttons for you | Optional |

## Turn one on

1. In setup's Permissions step, choose **Allow** next to the permission. System Settings opens at
   **Privacy & Security**.
2. Turn on Shotmaxxing in that list, then come back. The step updates by itself.
3. For Screen Recording, macOS lets Shotmaxxing use it once it reopens. Choose **Reopen Shotmaxxing**, and setup comes
   back to this step.

> [!NOTE]
> On macOS 15 and later the list is called **Screen & System Audio Recording**; on macOS 14 it's **Screen Recording**.

## It's on in System Settings, but setup still asks

macOS is matching the switch to an earlier copy of the app. Builds before 0.2.1 looked like a new app to macOS after
each update, and so does 0.2.1 the first time:

1. Open **System Settings → Privacy & Security**, then the list for that permission (Screen & System Audio Recording
   or Accessibility).
2. Select Shotmaxxing and remove it with the **−** button.
3. Back in Shotmaxxing, choose **Allow** again and turn the new entry on.

From 0.2.1 on, the Mac app in every release is signed with the same identity, so macOS keeps the permissions when
Shotmaxxing updates. If macOS forgets Screen Recording anyway, Shotmaxxing says so once at launch, and **Fix** opens
the step.

## On Windows

Windows records the screen without asking. Only its privacy switches for the microphone and the camera can stand in
the way: **Allow** opens **Settings → Privacy & security**. Turn on access, including for desktop apps, then come
back. The page checks live here too.
