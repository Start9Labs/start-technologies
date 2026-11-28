---
release-notes: false
---

- Local shared volumes and backup targets reject removal while an old cached mount remains active.
- Backup-target removal retains its storage reservation through completion if the requesting client disconnects.
