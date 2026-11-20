---
release-notes: true
---

- When too many login attempts block the web interface, **Sign in from the command line** shows a `startwrt-cli auth key add` command that enrolls the browser from the router without a password, so the interface keeps working during a password-guessing attack. `startwrt-cli auth key list` and `remove` manage enrolled keys.
