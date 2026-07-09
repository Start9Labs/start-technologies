# Prompt User to Create Admin Credentials

Most services need admin credentials before the user can sign in. The standard pattern pairs a `setupOnInit` watcher with a `setAdminPassword` action: the watcher surfaces a critical task when no password is stored, and the action — when the user runs it — generates, stores, and returns the credential. The same action handles later rotation.

## Solution

In `setupOnInit`, read the file model where the admin password lives. When it is unset, call `sdk.action.createOwnTask()` with severity `'critical'` pointing to the `setAdminPassword` action. The action is `sdk.Action.withoutInput`, `visibility: 'enabled'` so users can reach it for rotation, and its handler generates the password, writes it to the store, and returns it as a group result (username unmasked + copyable, password masked + copyable).

## Never roll your own password RNG

The random string comes from the SDK, always. It is not a top-level export — it lives under the `utils` namespace, and it takes a spec:

```typescript
import { utils } from '@start9labs/start-sdk'

const adminPassword = utils.getDefaultString({ charset: 'a-z,A-Z,0-9', len: 32 })
```

`getDefaultString()` with no argument **throws** — it resolves a `DefaultString`, which is either a literal string or a `RandomString` spec (`{ charset, len }`). If you find yourself reaching for `crypto.randomInt` and a hand-written alphabet, you are looking at the wrong name: it is `utils.getDefaultString`, not `getRandomString`.

When the credential is a field on an action **form** rather than something the handler mints, don't generate it in package code at all — hand the spec to the OS. `Value.text` takes a `RandomString` on `default` (pre-filled) or on `generate` (adds a "generate" button); see [Actions](actions.md#generating-values-in-a-form).

The shape gives you:

- **One source of truth.** The action is the only place that generates and stores; the init watcher only decides whether to surface the task.
- **Rotation for free.** Re-running the action overwrites the stored password and returns the new one — the same action covers first-set and reset.
- **Idempotent inits.** Task creation is idempotent on its replay key, so `setupOnInit` can run on every container rebuild without spamming tasks.

When the upstream service requires the password to be applied via CLI or API (rather than read from the store at startup), wrap the work in `sdk.SubContainer.withTemp()` inside the action handler and run the upstream command before returning — see the [Reset a Password](recipe-reset-password.md) recipe for the temp-subcontainer shape.

> [!WARNING]
> Do not hand-write a credential into the app's config file in a format you have not confirmed. Many apps store web passwords as **salted PBKDF2 or bcrypt** values with app-specific framing, not as a bare hash you can compute and drop into a config key — and writing the wrong format does not error, it silently rejects every login. Prefer applying the password through the app's **own** CLI/API (the temp-subcontainer shape above). If you must write the config directly, first confirm the exact on-disk format by setting a password through the app once and reading back what it wrote. Either way, **verify a real login succeeds before shipping** — a credential flow that has never been logged into is not done.

**Reference:** [Initialization](init.md) · [Tasks](tasks.md) · [Actions](actions.md)

## Examples

See `startos/init/` and `startos/actions/` in: [canary](https://github.com/Start9-Community/canary-startos) (cleanest reference — `watchCredentials.ts` + `setAdminPassword.ts`), [openclaw](https://github.com/Start9-Community/openclaw-startos) (`setPassword.ts`), [vaultwarden](https://github.com/Start9Labs/vaultwarden-startos) (`admin-token.ts`), [bisq](https://github.com/Start9-Community/bisq-startos), [helipad](https://github.com/Start9Labs/helipad-startos), [btcpayserver](https://github.com/Start9Labs/btcpayserver-startos), [lnbits](https://github.com/Start9Labs/lnbits-startos), [actual-budget](https://github.com/Start9Labs/actual-budget-startos), [gitea](https://github.com/Start9Labs/gitea-startos) (uses `withInput` to also take username/email; generates the password server-side).
