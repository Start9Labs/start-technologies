---
user-facing: true
---

- Keep daemon and subcontainer command environment values out of process command lines, splitting large environments into bounded chunks.
- Preserve multiline image environment values in packages built with start-cli 2.6.0 or later.
- Require StartOS 0.4.1 or later for packages built with this SDK.
