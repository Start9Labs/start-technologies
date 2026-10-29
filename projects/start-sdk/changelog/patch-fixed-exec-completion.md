---
user-facing: true
---

- Prevent fast subcontainer commands from hanging service health checks and shutdown. Honor command deadlines and cancellation while stdin is pending, and capture output through stream closure.
