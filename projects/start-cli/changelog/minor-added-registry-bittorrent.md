---
release-notes: true
---

- Download registry packages and OS assets through BitTorrent when available, with HTTP fallback and an HTTP-only option. CLI transfers upload no asset data.
- Add external BitTorrent sources to published assets with `--torrent`.
- Control sharing of compatible installed S9PKs and stored OS images on a StartOS server with `server bittorrent set-seeding`; the preference applies live and persists across restarts without extra payload copies or extended retention.
