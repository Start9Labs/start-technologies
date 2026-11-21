---
release-notes: true
---

- Rate-limit web UI logins by whether the client is on a private network, so password guessing from the internet no longer blocks logins over WireGuard. Each command-line login over localhost also has its own limit, and the login log names the client's real IP address.
- Check login passwords without blocking other work, so password guessing no longer delays port-mapping requests from StartOS.
