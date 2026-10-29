---
user-facing: true
---

- Replace the `waiting` health result with `waiting: true` on `starting` or `loading`. Waiting checks show a clock, and every status can display a nonempty custom message verbatim. Update health checks returning `result: 'waiting'` to use `result: 'starting', waiting: true`, and remove `waiting` entries from `statusTrigger` interval maps.
