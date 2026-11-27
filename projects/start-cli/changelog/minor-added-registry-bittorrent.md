---
release-notes: true
---

- Opt into BitTorrent downloads for registry packages and OS assets with `--bittorrent true` (default off). When enabled, advertised torrents are preferred with HTTP fallback. CLI transfers upload no asset data.
- Add external BitTorrent sources to published assets with `--torrent`. Private S9PK peers exchange encrypted payloads; downloaded archives remain plaintext.
- Control sharing of compatible installed S9PKs and stored OS images on a StartOS server with `server bittorrent set-preferences`; the preference applies live and persists across restarts without extra payload copies or extended retention. Use `server bittorrent set-address-enabled` and `server bittorrent set-gua-wan` to select incoming addresses.
