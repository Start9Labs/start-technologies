# Reset a Password

When users lose their admin password, they need a way to generate a new one. A reset action creates a temporary subcontainer, runs the app's password-reset command, and returns the new credentials. This works whether the service is running or stopped, depending on the app.

## Solution

Create an action with `sdk.Action.withoutInput()` that generates a new password using `utils.getDefaultString({ charset, len })`. Use `sdk.SubContainer.withTemp()` to spin up a temporary container, exec the app's password-reset command with `sub.execFail()`, then return the password as a masked, copyable result. For multi-user apps, use `sdk.Action.withInput()` with `Value.dynamicSelect` to query the running app for admin users and let the user choose which to reset.

**Reference:** [Actions](actions.md)

## Examples

See `startos/actions/` in: [uptime-kuma](https://github.com/Start9Labs/uptime-kuma-startos), [jitsi](https://github.com/Start9Labs/jitsi-startos), [filebrowser](https://github.com/Start9Labs/filebrowser-startos), [gitea](https://github.com/Start9Labs/gitea-startos), [nextcloud](https://github.com/Start9Labs/nextcloud-startos), [open-webui](https://github.com/Start9Labs/open-webui-startos), [ride-the-lightning](https://github.com/Start9Labs/ride-the-lightning-startos), [synapse](https://github.com/Start9Labs/synapse-startos), [immich](https://github.com/Start9Labs/immich-startos), [vaultwarden](https://github.com/Start9Labs/vaultwarden-startos)
