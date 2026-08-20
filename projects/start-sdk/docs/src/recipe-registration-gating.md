# Gate User Registration

Multi-user services often need registration enabled briefly (for the admin to create their account) then disabled to prevent unauthorized signups. A toggle action flips the setting and dynamically updates its own label to reflect the current state — "Enable Signups" vs "Disable Signups."

## Solution

Use `sdk.Action.withoutInput()` with an async metadata function (not a static object). The metadata reads the current registration state from a file model and dynamically sets the action name ("Enable Signups" vs "Disable Signups"), description, and warning. The handler reads the same state and flips the boolean. Pair with an `'important'` severity task on install reminding the user to disable registrations after creating their admin account.

**Reference:** [Actions](actions.md) · [File Models](file-models.md)

## Examples

See `startos/actions/` in: [gitea](https://github.com/Start9Labs/gitea-startos), [synapse](https://github.com/Start9Labs/synapse-startos), [vaultwarden](https://github.com/Start9Labs/vaultwarden-startos)
