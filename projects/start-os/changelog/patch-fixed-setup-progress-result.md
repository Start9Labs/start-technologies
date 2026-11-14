---
release-notes: false
---

- Setup progress reports the setup operation's failure rather than completing at 100%. The setup wizard waits for the operation's result before finalizing setup and reconnects progress observation without restarting setup work. Concurrent setup requests cannot start work that was rejected.
