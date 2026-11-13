---
release-notes: false
---

- Report actual journal process outcomes through typed log observations while preserving legacy log streams and reject out-of-range or fractional boot selectors.
- Cancel portal and initialization/setup log observation and reconnect waits when their views are disposed.
- Keep log streams active while their initial backlog waits for database synchronization.
