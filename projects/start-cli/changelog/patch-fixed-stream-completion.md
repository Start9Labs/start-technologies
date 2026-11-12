---
release-notes: false
---

- Report interrupted or failed CLI log, terminal, initialization, update and sideload streams as errors instead of successful completion. System update and sideload progress wait for the actual operation result, including failures after progress reaches 100%.
- Localize sideload progress, version selection and install errors in all supported languages.
- Exit promptly when terminal attachments fail while local input remains open.
- Keep log follows active while receiving the initial backlog.
