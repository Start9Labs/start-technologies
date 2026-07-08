# Tasks

Tasks are notifications that appear in the StartOS UI prompting the user to run a specific [action](./actions.md). They are commonly used to surface important information after install or restore, request required configuration, or coordinate setup with dependency services.

## Own Tasks

Use `sdk.action.createOwnTask()` to prompt the user to run one of your service's own actions.

```typescript
await sdk.action.createOwnTask(effects, setAdminPassword, 'critical', {
  reason: i18n('Set the admin password before signing in'),
})
```

### Parameters

| Parameter  | Type                                      | Description                                  |
| ---------- | ----------------------------------------- | -------------------------------------------- |
| `effects`  | `Effects`                                 | Provided by the calling context              |
| `action`   | `ActionDefinition`                        | The action to prompt the user to run         |
| `severity` | `'critical' \| 'important' \| 'optional'` | How urgently the task is surfaced in the UI  |
| `options`  | `{ reason: string }`                      | Human-readable explanation shown to the user |

### Severity Levels

- **critical** — Blocks the service from starting until the user completes the task. Use for essential setup like creating admin credentials or selecting a backend.
- **important** — Prominently displayed but does not block the service. Use for post-install reminders like disabling registrations.
- **optional** — Informational, least prominent.

## Common Patterns

### Prompt When Credentials Are Unset

The standard admin-credentials pattern: init reads the store and surfaces a critical task when the password is unset. Generation lives in the matching action, which covers both first-set and later rotation. The watcher runs on every init kind; the prompt is idempotent (see [Idempotency and `replayId`](#idempotency-and-replayid)), so a container rebuild after the password is set is a no-op:

```typescript
export const watchCredentials = sdk.setupOnInit(async effects => {
  const store = await storeJson.read().const(effects)

  if (!store?.adminPassword) {
    await sdk.action.createOwnTask(effects, setAdminPassword, 'critical', {
      reason: i18n('Set the admin password before signing in'),
    })
  }
})
```

See the [Prompt User to Create Admin Credentials](./recipe-admin-credentials.md) recipe for the matching action.

### Prompt for Required Configuration

Ask the user to configure something before the service can function:

```typescript
await sdk.action.createOwnTask(effects, manageSmtp, 'important', {
  reason: i18n('Configure email settings to enable notifications'),
})
```

## Dependency Tasks

Use `sdk.action.createTask()` to prompt the user to run an action on a dependency service. The action must be imported from the dependency's package.

```typescript
import { someAction } from 'dependency-package/startos/actions/someAction'

export const setDependencies = sdk.setupDependencies(async ({ effects }) => {
  await sdk.action.createTask(effects, 'dependency-id', someAction, 'critical', {
    input: {
      kind: 'partial',
      accept: [
        {
          /* one or more acceptable partial inputs */
        },
      ],
      set: {
        /* the value to pre-fill when none are accepted */
      },
    },
    when: { condition: 'input-not-matches', once: false },
    reason: i18n('Configure the dependency for use with this service'),
  })

  return {
    'dependency-id': {
      kind: 'running',
      versionRange: '>=1.0.0:0',
      healthChecks: ['dependency-id'],
    },
  }
})
```

### Parameters

| Parameter   | Type                                      | Description                            |
| ----------- | ----------------------------------------- | -------------------------------------- |
| `effects`   | `Effects`                                 | Provided by the calling context        |
| `packageId` | `string`                                  | The dependency's service ID            |
| `action`    | `ActionDefinition`                        | Imported from the dependency's package |
| `severity`  | `'critical' \| 'important' \| 'optional'` | How urgently the task is surfaced      |
| `options`   | `object`                                  | See below                              |

### Options

| Field      | Type                                                                         | Description                                                                                                              |
| ---------- | ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `input`    | `{ kind: 'partial', accept: Partial<InputSpec>[], set: Partial<InputSpec> }` | `accept` lists the partial inputs that satisfy the task; `set` pre-fills the action's input form when none of them match |
| `when`     | `{ condition: 'input-not-matches', once: boolean }`                          | Re-trigger until the action's input matches one of the `accept` values                                                   |
| `reason`   | `string`                                                                     | Human-readable explanation shown to the user                                                                             |
| `replayId` | `string` (optional)                                                          | Overrides the default idempotency key (see below)                                                                        |

With `condition: 'input-not-matches'`, the task is **satisfied** when the action's current input is a superset of **any** entry in `accept` (each entry is matched partially — only the fields you list must agree). When none match, the task is shown and the action form is pre-filled with `set`. Use multiple `accept` entries to tolerate several already-good configurations while still steering the user to one recommended value; for the common case where any value but one specific target is unacceptable, pass a single `accept` entry equal to `set`.

> [!NOTE]
> The dependency must be listed in your `package.json` so the action can be imported (e.g., `"synapse-startos": "file:../synapse-wrapper"`). See [Dependencies](./dependencies.md) for more on cross-service integration.

### Idempotency and `replayId`

Tasks are idempotent by default. The SDK computes a default `replayId` of `[package-id]:[action-id]`, so calling `createOwnTask` / `createTask` multiple times with the same action does **not** create duplicate tasks — subsequent calls are no-ops against the same replay key. You can safely re-run your init function on every container rebuild without accumulating stale tasks.

Provide a custom `replayId` only when you need to intentionally create multiple distinct tasks for the same action (e.g., one-per-peer setup prompts). Each unique `replayId` becomes a separate task.

To cancel a task programmatically, clear it by its replay key:

```typescript
await sdk.action.clearTask(effects, 'my-service:set-admin-password')
```
