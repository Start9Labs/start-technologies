---
release-notes: true
---

- Add optional BitTorrent downloads for marketplace packages and routine OS updates. When enabled, advertised torrents are preferred with HTTP fallback. **System → Software sharing** controls downloads and sharing compatible installed S9PKs and stored OS images; both default off. Incoming peers opt in through initially-off address groups, including LAN/private addresses, on a dynamically assigned external port, allocated only after the first address opt-in and released when incoming sharing is disabled. Saved address choices survive temporary discovery loss. A stopped settings watcher is reported on the page; restart StartOS to restore peer transfers. It shows the StartOS default gateway and reuses native Address Requirements and PCP/UPnP forwarding, without extra payload copies or extended retention.
