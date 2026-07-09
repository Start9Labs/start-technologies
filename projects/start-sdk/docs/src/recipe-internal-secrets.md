# Auto-Generate Internal Secrets

Many services need passwords or tokens that are generated once and used internally — database passwords, API secret keys, inter-container auth tokens. These are never shown to the user. Generate them at install time and store them in store.json for later consumption.

## Solution

In `setupOnInit`, check for `kind === 'install'` and generate random strings with `utils.getDefaultString({ charset, len })`. Write them to store.json via a file model. These secrets are consumed in `setupMain` as env vars or config file values — they are never shown to the user.

**Reference:** [Initialization](init.md) · [File Models](file-models.md)

## Examples

See `startos/init/` and `startos/fileModels/` in: [spliit](https://github.com/Start9Labs/spliit-startos), [ghost](https://github.com/Start9Labs/ghost-startos), [nextcloud](https://github.com/Start9Labs/nextcloud-startos), [immich](https://github.com/Start9Labs/immich-startos), [jitsi](https://github.com/Start9Labs/jitsi-startos), [mcaptcha](https://github.com/Start9Labs/mcaptcha-startos), [simplex](https://github.com/Start9Labs/simplex-startos), [vaultwarden](https://github.com/Start9Labs/vaultwarden-startos), [gitea](https://github.com/Start9Labs/gitea-startos), [synapse](https://github.com/Start9Labs/synapse-startos)
