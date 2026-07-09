# Run One-Time Setup on Install

Fresh installs often need one-time bootstrapping — generating passwords, seeding config file defaults, creating initial database records. The `setupOnInit` hook receives a `kind` parameter that tells you why initialization is running.

## Solution

In `setupOnInit`, check `kind === 'install'` and run one-time setup: generate passwords with `utils.getDefaultString({ charset, len })`, seed config file defaults with `fileModel.merge(effects, {})` (an empty merge fills every missing field from its `.catch()` default), and create tasks for user actions. For setup that should run on both install and restore but not container rebuild, check `kind !== null`. The four init kinds are `'install'`, `'update'`, `'restore'`, and `null`.

**Reference:** [Initialization](init.md) · [File Models](file-models.md)

## Examples

See `startos/init/` in: [spliit](https://github.com/Start9Labs/spliit-startos), [ghost](https://github.com/Start9Labs/ghost-startos), [nextcloud](https://github.com/Start9Labs/nextcloud-startos), [immich](https://github.com/Start9Labs/immich-startos), [gitea](https://github.com/Start9Labs/gitea-startos), [synapse](https://github.com/Start9Labs/synapse-startos), [simplex](https://github.com/Start9Labs/simplex-startos), [mcaptcha](https://github.com/Start9Labs/mcaptcha-startos), [vaultwarden](https://github.com/Start9Labs/vaultwarden-startos)
