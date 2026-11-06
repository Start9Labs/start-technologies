---
release-notes: false
---

- Add `server set-log-level <error|warn|info|debug|trace>` to change verbosity for `start-core`/`start-os` in the running daemon until it restarts. Dependency logs keep their startup filter; saved configuration remains unchanged.
