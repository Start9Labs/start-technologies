---
release-notes: false
---

- Reloading or reconnecting the web interface downloads only what changed since it was last open, rather than the whole server state.
- The web interface writes less to browser storage while services and registry listings update.
- Each page receives only the server state it shows, and a service's Marketplace details, including other versions, stay current while open.
