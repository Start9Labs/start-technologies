---
release-notes: true
---

- The admin password set during setup is encrypted in the browser before it is sent to the router, in both the setup wizard and the Create admin password page.
- LuCI opens only over HTTPS, so signing in to it no longer sends the admin password unencrypted: plain-HTTP requests to it redirect to HTTPS, and it no longer answers on port 8080.
