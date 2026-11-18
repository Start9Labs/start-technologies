---
release-notes: true
---

- Services can no longer reach each other's containers directly; they connect through StartOS, as dependencies already do. Each service container also runs under its own range of user and group ids.
- A server holds up to 1024 installed services. Installing beyond that is refused before anything is downloaded.
