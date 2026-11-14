---
release-notes: false
---

- Password changes continue after client disconnection, preserve concurrent server-name changes and keep SSH/console credentials synchronized after account commit.
- Ordinary logins avoid repeated password verification when shadow credentials are already synchronized.
