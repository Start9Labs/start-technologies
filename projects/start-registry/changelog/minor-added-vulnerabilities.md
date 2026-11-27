---
release-notes: false
---

- Package and StartOS indexes carry signed vulnerabilities: an affected version range, when it is exploitable, its impact, and whether stopping the service is recommended. `package.vulnerability.add`/`remove` follow package publishing authorization; `os.vulnerability.add`/`remove` are admin-only.
