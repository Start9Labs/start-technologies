---
release-notes: true
---

- Prevent subcontainer images from redirecting hostname and hosts-file setup into host files through filesystem symlinks.
- Reject symlink targets when the host connects or reconnects to a service's runtime socket.
