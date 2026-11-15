# Security

## Reporting a vulnerability

Please report security problems privately through
[GitHub's private vulnerability reporting](https://github.com/subh05sus/shotmaxxing/security/advisories/new), not as a
public issue. Include what you found, how to reproduce it, and what an attacker could do with it.

You can expect a first reply within a few days. Once a fix is ready, we'll agree a disclosure date with you and credit
you in the release notes if you'd like.

## What matters most

Shotmaxxing records screens and lets local AI agents drive recordings, so these areas get the closest attention:

- **The local control API.** It listens only on a Unix socket that only the current user can open (macOS) or on a
  named pipe whose access list admits only the current user, from this computer (Windows). The app learns which
  process connected from the system, not from anything the caller says. Anything that reaches it from another user,
  another machine or a web page is a vulnerability.
- **Agent actions.** The person allows each agent session, and then each take, on the take card before anything is
  seen, recorded or performed. Takes never type into password fields, and pressing Esc three times or moving the
  mouse always stops them. A way around any of these is a vulnerability.
- **Privacy.** Recordings, transcripts and project files never leave the machine. Anything that sends them elsewhere
  without the person's action is a vulnerability.
- **Project files.** Opening a `.smx` project must never run code or write outside the project folder.

## Supported versions

Until 1.0, only the latest release receives security fixes.
