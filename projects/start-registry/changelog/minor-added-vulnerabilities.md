---
release-notes: false
---

- Package and StartOS indexes carry signed vulnerabilities: an affected version range, when it is exploitable, its impact, and whether stopping the service is recommended. `package.vulnerability.add`/`remove` follow package publishing authorization, and only an admin or a current signer can replace or remove a published vulnerability, including through whole-package removal. `os.vulnerability.add`/`remove` are admin-only.
