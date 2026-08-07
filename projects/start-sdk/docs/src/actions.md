# Actions

Actions are user-triggered operations that appear in the StartOS UI for your service. They can display information, accept user input, modify configuration, and more.

## Action Without Input

The simplest action type does its work and returns a result to display. The canonical "set admin password" action generates a random password, writes it to the store, and returns the new credential — the same action serves first-set (surfaced by a critical task on install) and later rotation:

```typescript
import { utils } from '@start9labs/start-sdk'
import { i18n } from '../i18n'
import { sdk } from '../sdk'
import { storeJson } from '../fileModels/store.json'

export const setAdminPassword = sdk.Action.withoutInput(
  // ID
  'set-admin-password',

  // Metadata
  async () => ({
    name: i18n('Set Admin Password'),
    description: i18n('Generate a new random password for the admin account. Replaces any existing password.'),
    warning: null,
    allowedStatuses: 'any', // 'any', 'only-running', 'only-stopped'
    group: null,
    visibility: 'enabled', // 'enabled', 'disabled', 'hidden'
    access: 'user', // 'public' | 'dependent' | 'user' — who may invoke directly via effects.action.run (default 'user')
  }),

  // Handler
  async ({ effects }) => {
    const adminPassword = utils.getDefaultString({
      charset: 'a-z,A-Z,0-9',
      len: 32,
    })
    await storeJson.merge(effects, { adminPassword })

    return {
      version: '1',
      title: i18n('Login Credentials'),
      message: i18n('Use these credentials to sign in.'),
      result: {
        type: 'group',
        value: [
          {
            type: 'single',
            name: i18n('Username'),
            description: null,
            value: 'admin',
            masked: false,
            copyable: true,
            qr: false,
          },
          {
            type: 'single',
            name: i18n('Password'),
            description: null,
            value: adminPassword,
            masked: true,
            copyable: true,
            qr: false,
          },
        ],
      },
    }
  },
)
```

The action is paired with a `setupOnInit` watcher that surfaces a critical task when no password is stored — generation, storage, and display all live in this one handler, so first-set and rotation share a single code path. See [Prompt User to Create Admin Credentials](./recipe-admin-credentials.md).

### Controlling Access

The optional **`access`** field on the metadata controls who may invoke the action **directly** via `effects.action.run({ packageId, actionId, input })`:

- `'user'` (default) — only the user; another service must request it through a task (`effects.action.createTask(...)`).
- `'dependent'` — only services that declare this package as a current dependency.
- `'public'` — any installed package.

`access` is independent of `visibility` (whether the action is shown/enabled) and `allowedStatuses` (which run states permit it); a direct cross-package run is rejected if `access` denies the caller.

## Registering Actions

All actions must be registered in `actions/index.ts`:

```typescript
import { sdk } from '../sdk'
import { setAdminPassword } from './setAdminPassword'

export const actions = sdk.Actions.of().addAction(setAdminPassword)
```

## Result Types

Actions return structured results that the StartOS UI renders for the user.

### Single Value

```typescript
result: {
  type: 'single',
  name: 'API Key',
  description: null,
  value: 'abc123',
  masked: true,
  copyable: true,
  qr: false,
}
```

### Group of Values

```typescript
result: {
  type: 'group',
  value: [
    { type: 'single', name: 'Username', description: null, value: 'admin', masked: false, copyable: true, qr: false },
    { type: 'single', name: 'Password', description: null, value: 'secret', masked: true, copyable: true, qr: false },
  ],
}
```

## Tasks

Actions can be surfaced to users as tasks — notifications that prompt them to run a specific action at the right time. See [Tasks](./tasks.md) for details.

## Implementation Examples

### Auto-Generate Passwords

The standard shape for password actions: the handler generates the password with `utils.getDefaultString({ charset, len })`, writes it where the service reads it from, and returns it as a masked, copyable result. Server-side generation produces strong passwords and means the same action covers first-set and rotation. The primary example above (`setAdminPassword`) is the canonical shape — see also the [Reset a Password](./recipe-reset-password.md) recipe for variants that apply the new password through the upstream service's CLI or API.

### Registration-Gated Services

Some services require that "registrations" or "signups" be enabled for users to create accounts. This creates a security tension: the service must be open for the admin to register, but should be locked down after.

The recommended pattern:

1. **Start with registrations enabled** in the initial config.
2. **Create an important task** in `setupOnInit` advising the user to disable registrations after creating their admin account.
3. **Provide a toggle action** that reads the current registration state, flips it, and writes back.

```typescript
// In init/taskDisableRegistrations.ts
export const taskDisableRegistrations = sdk.setupOnInit(async (effects, kind) => {
  if (kind !== 'install') return
  await sdk.action.createOwnTask(effects, toggleRegistrations, 'important', {
    reason: 'After creating your admin account, disable registrations to prevent unauthorized signups.',
  })
})

// In actions/toggleRegistrations.ts
import { configToml } from '../fileModels/config.toml'

export const toggleRegistrations = sdk.Action.withoutInput(
  'toggle-registrations',
  async ({ effects }) => {
    const allowed = await configToml.read(c => c.allow_registration).const(effects)
    return {
      name: allowed ? i18n('Disable Registrations') : i18n('Enable Registrations'),
      description: allowed ? i18n('Registrations are currently enabled. Run this action to disable them.') : i18n('Registrations are currently disabled. Run this action to enable them.'),
      warning: allowed ? null : i18n('Anyone with your URL will be able to create an account.'),
      allowedStatuses: 'any',
      group: null,
      visibility: 'enabled',
    }
  },
  async ({ effects }) => {
    const allowed = await configToml.read(c => c.allow_registration).const(effects)
    await configToml.merge(effects, { allow_registration: !allowed })
  },
)
```

## Action With Input

For actions that accept user input, use `sdk.Action.withInput()` with an `InputSpec` form, a prefill function, and a handler:

```typescript
import { sdk } from '../sdk'
import { configFile } from '../fileModels/config'
import { i18n } from '../i18n'
const { InputSpec, Value } = sdk

const inputSpec = InputSpec.of({
  timeout: Value.number({
    name: i18n('Session Timeout'),
    description: i18n('How long before idle sessions expire'),
    required: false,
    default: 30,
    min: 1,
    max: 1440,
    step: 1,
    integer: true,
    units: 'minutes',
  }),
})

export const configure = sdk.Action.withInput(
  'configure',
  {
    name: i18n('Configure'),
    description: i18n('Adjust service settings'),
    warning: null,
    allowedStatuses: 'any',
    group: null,
    visibility: 'enabled',
  },
  inputSpec,
  // Prefill form with current values
  async ({ effects }) => {
    const current = await configFile.read(c => c.timeout).once()
    return { timeout: current }
  },
  // Handler — write new values
  async ({ effects, input }) => {
    await configFile.merge(effects, { timeout: input.timeout })
  },
)
```

The five arguments to `withInput` are: action ID, metadata (static object or async function), input spec, prefill function, and handler.

### Generating Values in a Form

When a form field holds a secret, don't generate it in package code. `Value.text` accepts a `RandomString` spec — `{ charset, len }` — in two places, and StartOS does the generating:

```typescript
password: Value.text({
  name: i18n('Password'),
  description: i18n('Leave as generated, or choose your own'),
  required: true,
  masked: true,
  // Pre-fill the field with a fresh random value each time the form opens
  default: { charset: 'a-z,A-Z,0-9', len: 32 },
  // …and/or render a "generate" button that refills it on demand
  generate: { charset: 'a-z,A-Z,0-9', len: 32 },
}),
```

`default` also takes a plain string when you want a fixed literal. The same `RandomString` shape is what [`utils.getDefaultString`](recipe-admin-credentials.md#never-roll-your-own-password-rng) resolves in a `withoutInput` handler — between the two, package code never needs its own random-string generator.

### Editing Records the Service Owns

Some actions edit records that live inside the service — rows in its database, or whatever its own API exposes — rather than in a StartOS file model. The form is then a _view_ of state that can change behind it: the user may edit the same records in the service's own admin UI, or from a second StartOS session, between opening the form and saving it.

Carry the correlation state you need in hidden fields. `Value.hidden(parser)` accepts any zod type, works at any level of the spec — the root, or inside a list item's object spec — renders nothing, and round-trips from the prefill function through to the handler's `input`:

```typescript
import { z } from '@start9labs/start-sdk'

const inputSpec = InputSpec.of({
  // The record ids that existed when this form was rendered.
  knownIds: Value.hidden(z.array(z.string()).nullish()),
  items: Value.list(
    sdk.List.obj(
      { name: i18n('Items') },
      {
        displayAs: '{{name}}',
        spec: InputSpec.of({
          // Absent on a row the user just added (→ create); present on one
          // read back from the service (→ update in place).
          id: Value.hidden(z.string().nullish()),
          name: Value.text({ name: i18n('Name'), required: true, default: null }),
        }),
      },
    ),
  ),
})
```

Two rules then keep the action from destroying work it never saw.

**Correlate by the service's id, not by a user-visible field.** With the id in hand the handler can tell an edit from a create. Matching on the name instead makes a rename read as "delete the old record, create a new one" — which duplicates on rename and orphans on removal.

**Scope deletion to what the form actually rendered.** A handler that deletes every record missing from the submitted list will also delete records created after the form opened. Delete only within `knownIds`:

```typescript
export const editItems = sdk.Action.withInput(
  'edit-items',
  {
    name: i18n('Edit Items'),
    description: i18n("Edit the service's records"),
    warning: i18n('Removing a row deletes that record.'),
    // It reads and writes the live service, so it needs the service up.
    allowedStatuses: 'only-running',
    group: null,
    visibility: 'enabled',
  },
  inputSpec,
  // Prefill from the service, recording the ids we are about to show
  async ({ effects }) => {
    const records = await serviceApi<Record[]>('/records')
    return {
      knownIds: records.map(r => r.id),
      items: records.map(r => ({ id: r.id, name: r.name })),
    }
  },
  // Create and update, then delete only within what we rendered
  async ({ effects, input }) => {
    const submitted = new Set<string>()
    for (const row of input.items) {
      if (row.id) {
        submitted.add(row.id)
        await serviceApi(`/records/${row.id}`, { method: 'PUT', body: row })
      } else {
        await serviceApi('/records', { method: 'POST', body: row })
      }
    }
    const rendered = new Set(input.knownIds ?? [])
    for (const record of await serviceApi<Record[]>('/records')) {
      if (rendered.has(record.id) && !submitted.has(record.id)) {
        await serviceApi(`/records/${record.id}`, { method: 'DELETE' })
      }
    }
  },
)
```

Hidden fields travel to the client with the rest of the form and are not user-editable, which makes them right for correlation state and wrong for anything secret.

Two further cautions for this shape of action:

- **Reconcile in the handler, not in a startup oneshot.** A oneshot in `main` that re-applies the form's records on every boot fights the service's own UI — it overwrites whatever the user changed there between restarts.
- **Decide what a rejected record should do to the rest.** If the service validates on write, one bad record in an all-or-nothing loop fails the whole save, so an unrelated edit in the same submission never lands. Either apply rows independently and report the failures together, or document that the form is all-or-nothing.

## Conventions

### Wrap User-Facing Strings in `i18n()`

Every string that a user will see — action `name`, `description`, `warning`, `reason` on tasks, messages on health checks and action results — must be wrapped in `i18n()`. Raw strings bypass translation and leak English into non-English locales. The existing examples on this page illustrate the pattern: `name: i18n('Configure SMTP')`, not `name: 'Configure SMTP'`.

Thrown errors are the exception. `throw new Error(...)` messages are developer-facing diagnostics that surface in logs and stack traces, not translated UI copy — leave them as plain strings and do **not** wrap them in `i18n()`.

### Don't `as const` What the SDK Already Types

Action metadata and results are contextually typed by the SDK's own signatures — `version: '1'` is declared as the literal `'1'`, and `visibility`, `allowedStatuses`, `access`, and `type` are unions, not `string`. Write the literal and stop:

```typescript
// GOOD — the SDK narrows these for you
return { version: '1', title: i18n('Login Credentials'), ... }

// NOISE — asserts something the compiler already knows
return { version: '1' as const, ... }
```

`tsc` passes either way, which is why the assertions spread by copy-paste. They aren't load-bearing anywhere in an action file; drop them when you see them. (Distinct from an `as` **cast**, which claims the compiler is wrong — reach for that only when it actually is.)

### Mirror File-Model Keys in InputSpec When Appropriate

When an action's job is "set these fields on this file-model section," name the `InputSpec` keys to match the file-model keys exactly — same casing, same spelling. The prefill and handler collapse to one-liners:

```typescript
// fileModels/config.json uses uppercase snake_case keys under MEMPOOL
const spec = InputSpec.of({
  BLOCKS_SUMMARIES_INDEXING: Value.toggle({
    /* ... */
  }),
  GOGGLES_INDEXING: Value.toggle({
    /* ... */
  }),
  AUDIT: Value.toggle({
    /* ... */
  }),
  CPFP_INDEXING: Value.toggle({
    /* ... */
  }),
})

sdk.Action.withInput(
  'configure-indexing',
  {
    /* metadata */
  },
  spec,
  async ({ effects }) => configJson.read(c => c.MEMPOOL).once(),
  async ({ effects, input }) => configJson.merge(effects, { MEMPOOL: input }),
)
```

Benefits:

- Prefill and write collapse to direct pass-throughs — no manual object-literal mapping on either side.
- If the file model later adds or removes a field the action exposes, TypeScript flags the mismatch instead of silently dropping it.

**When not to mirror:** if the action transforms values, combines multiple inputs, writes to multiple sections, or writes to a section where the file-model keys aren't a good user-facing vocabulary. In those cases, use human-readable camelCase input names and do the mapping in the handler.

> [!NOTE]
> Action prefills use `.once()`, not `.const(effects)`. `.const()` sets up a reactive watcher meant for `setupMain` — it's wasted overhead in a prefill, which is a one-shot read at the moment the form opens.

## SMTP Configuration

The SDK provides a built-in SMTP input specification for managing email credentials. This supports three modes: disabled, system SMTP (from StartOS settings), or custom SMTP with provider presets (Gmail, Amazon SES, SendGrid, Mailgun, Proton Mail, or custom).

### 1. Add SMTP to store.json.ts

Use the SDK's `smtpShape` zod schema in your store's shape definition. See [File Models](./file-models.md) for more on file model patterns.

```typescript
import { FileHelper, smtpShape, z } from '@start9labs/start-sdk'
import { sdk } from '../sdk'

const shape = z.object({
  adminPassword: z.string().optional(),
  secretKey: z.string().optional(),
  smtp: smtpShape,
})

export const storeJson = FileHelper.json({ base: sdk.volumes.main, subpath: './store.json' }, shape)
```

### 2. Create the manageSmtp Action

Use `smtpPrefill()` in the prefill function to bridge between the stored `SmtpSelection` type and the input spec's expected type. These types represent the same data but are structurally different in TypeScript (the store uses a flat union, the input spec uses a distributed discriminated union), so `smtpPrefill()` handles the conversion.

```typescript
import { smtpPrefill } from '@start9labs/start-sdk'
import { i18n } from '../i18n'
import { storeJson } from '../fileModels/store.json'
import { sdk } from '../sdk'

const { InputSpec } = sdk

export const inputSpec = InputSpec.of({
  smtp: sdk.inputSpecConstants.smtpInputSpec,
})

export const manageSmtp = sdk.Action.withInput(
  'manage-smtp',

  async ({ effects }) => ({
    name: i18n('Configure SMTP'),
    description: i18n('Add SMTP credentials for sending emails'),
    warning: null,
    allowedStatuses: 'any',
    group: null,
    visibility: 'enabled',
  }),

  inputSpec,

  // Pre-fill form with current values
  async ({ effects }) => ({
    smtp: smtpPrefill(await storeJson.read(s => s.smtp).const(effects)),
  }),

  // Save to store
  async ({ effects, input }) => storeJson.merge(effects, { smtp: input.smtp }),
)
```

### 3. Register the Action

```typescript
import { sdk } from '../sdk'
import { setAdminPassword } from './setAdminPassword'
import { manageSmtp } from './manageSmtp'

export const actions = sdk.Actions.of().addAction(setAdminPassword).addAction(manageSmtp)
```

### 4. Use SMTP Credentials at Runtime

In your `main.ts`, resolve the SMTP credentials based on the user's selection:

```typescript
import { T } from '@start9labs/start-sdk'

export const main = sdk.setupMain(async ({ effects }) => {
  const store = await storeJson.read().const(effects)

  // Resolve SMTP credentials based on selection
  const smtp = store?.smtp
  let smtpCredentials: T.SmtpValue | null = null

  if (smtp?.selection === 'system') {
    // Use system-wide SMTP from StartOS settings
    smtpCredentials = await sdk.getSystemSmtp(effects).const()
    if (smtpCredentials && smtp.value.customFrom) {
      smtpCredentials.from = smtp.value.customFrom
    }
  } else if (smtp?.selection === 'custom') {
    // Use custom SMTP credentials from the selected provider
    const { host, from, username, password, security } = smtp.value.provider.value
    smtpCredentials = {
      host,
      port: Number(security.value.port),
      from,
      username,
      password: password ?? null,
      security: security.selection,
    }
  }
  // If smtp.selection === 'disabled', smtpCredentials remains null

  // Pass to config generation
  const config = generateConfig({
    smtp: smtpCredentials,
    // ... other config
  })

  // ...
})
```

### 5. Initialize with SMTP Disabled

In `init/seedFiles.ts`, set the default SMTP state alongside any internal-only secrets the service needs. The admin password is set by the `setAdminPassword` action when the user runs its critical task (see [Prompt User to Create Admin Credentials](./recipe-admin-credentials.md)):

```typescript
await storeJson.merge(effects, {
  secretKey: utils.getDefaultString({ charset: 'a-z,A-Z,0-9', len: 64 }),
  smtp: { selection: 'disabled', value: {} },
})
```

### T.SmtpValue Type

The resolved SMTP credentials (returned by `sdk.getSystemSmtp()`) have this structure:

```typescript
interface SmtpValue {
  host: string
  port: number
  from: string
  username: string
  password: string | null | undefined
  security: 'starttls' | 'tls'
}
```

### SmtpSelection Type

The stored SMTP selection (from `smtpShape`) has this structure:

```typescript
type SmtpSelection =
  | { selection: 'disabled'; value: Record<string, never> }
  | { selection: 'system'; value: { customFrom?: string | null } }
  | {
      selection: 'custom'
      value: {
        provider: {
          selection: string // "gmail", "ses", "sendgrid", etc.
          value: {
            host: string
            from: string
            username: string
            password?: string | null
            security: {
              selection: 'tls' | 'starttls'
              value: { port: string }
            }
          }
        }
      }
    }
```
