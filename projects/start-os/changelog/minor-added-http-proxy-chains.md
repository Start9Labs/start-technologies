---
release-notes: true
---

- Add independent HTTP proxy chains for each binding and gateway. Choose profiles from installed proxy services for bot protection or request filtering, with optional public-client-only filtering on network gateways. Profile names fall back to each provider's default language. CLI commands publish provider profiles, read attachments and select gateway chains. URL-plugin gateways match their originating service container; unavailable stages block applicable traffic instead of bypassing protection. Uninstalling a provider removes its stages from every chain, preserving the remaining stages and gateway settings.
