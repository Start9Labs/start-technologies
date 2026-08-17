# Set Up SMTP / Email

Services that send email (notifications, password resets, invites) need SMTP configuration. The standard StartOS pattern offers three modes: disabled (no email), system (uses the StartOS system SMTP if configured), and custom (user provides their own SMTP server). The SDK provides built-in constructs for the entire flow.

## Solution

Add the SDK's built-in `smtpShape` to your store.json file model. Create a `manageSmtp` action using `sdk.Action.withInput()` with `sdk.inputSpecConstants.smtpInputSpec` — this provides the standard three-mode UI (disabled/system/custom). In `setupOnInit`, default SMTP to disabled. In `setupMain`, read the SMTP config and pass credentials as environment variables or write them to the app's config file.

**Reference:** [Actions](actions.md) · [File Models](file-models.md) · [Main](main.md)

## Examples

See `startos/actions/` and `startos/fileModels/` in: [ghost](https://github.com/Start9Labs/ghost-startos), [gitea](https://github.com/Start9Labs/gitea-startos), [immich](https://github.com/Start9Labs/immich-startos), [synapse](https://github.com/Start9Labs/synapse-startos), [vaultwarden](https://github.com/Start9Labs/vaultwarden-startos)
