---
release-notes: false
---

- When WAN addresses change, keep the port forward or SNI route already at the destination and log any conflicting forward or route that is dropped, instead of silently overwriting entries or creating overlapping port ranges.
