---
release-notes: true
---

- Services can fulfill dependencies on other package IDs. Switch providers in each service's dependency dialog, or lock one to require that exact package. Unlocked dependencies fall back to the sole compatible alternative; missing locked providers retain their name and icon and link to their exact Marketplace listing.
- Required setup tasks are re-evaluated when a service changes providers or re-enables an integration.
