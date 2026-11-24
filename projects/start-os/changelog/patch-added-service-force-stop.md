---
release-notes: true
---

- **Force stop services that are stuck stopping.** The web interface's confirmation warns about data loss and an inconsistent service state. The button appears after 30 seconds by default, configurable with `force-stop-delay-seconds` in `config.yaml`. Also available through `start-cli package force-stop` with the current stop-episode deadline.
