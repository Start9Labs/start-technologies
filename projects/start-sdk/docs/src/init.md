# Initialization

`setupOnInit` runs during container initialization. The `kind` parameter indicates why init is running:

| Kind        | When                              | Use For                                                                                                              |
| ----------- | --------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `'install'` | Fresh install                     | Generate internal secrets, seed file-model defaults, create critical tasks for user setup actions, bootstrap via API |
| `'update'`  | After a package version upgrade   | Re-apply config, handle post-migration setup                                                                         |
| `'restore'` | Restoring from backup             | Re-register triggers; credentials are already present from the restored store                                        |
| `null`      | Container rebuild, server restart | Register long-lived triggers (e.g., `.const()` watchers)                                                             |

## Init Kinds

### Install Only

For one-time setup that generates new state. Internal-only secrets (DB password, JWT secret, etc.) **are** generated here, because no user interaction is involved:

```typescript
export const seedFiles = sdk.setupOnInit(async (effects, kind) => {
  if (kind !== 'install') return

  // Internal secret consumed by setupMain — never shown to the user
  await storeJson.merge(effects, {
    jwtSecret: utils.getDefaultString({ charset: 'a-z,A-Z,0-9', len: 64 }),
  })
})
```

User-facing admin credentials follow a different pattern — see [Watch State and Prompt](#watch-state-and-prompt-the-admin-credentials-pattern) below.

### Restore

For setup that should also run when restoring from backup (but not on container rebuild):

```typescript
export const reRegisterWebhook = sdk.setupOnInit(async (effects, kind) => {
  if (kind === null) return // Skip on container rebuild

  // Runs on both install and restore — e.g. re-register a webhook with an
  // upstream service that was issued against a hostname that may have changed.
  await registerWebhook(effects)
})
```

### Always (Container Lifetime)

For registering `.const()` triggers that need to persist for the container's lifetime. These re-register on container rebuild:

```typescript
export const registerWatchers = sdk.setupOnInit(async (effects, kind) => {
  // Runs on install, restore, AND container rebuild

  // Register a watcher that lives for the container lifetime
  someConfig.read(c => c.setting).const(effects)

  // Install-specific setup
  if (kind === 'install') {
    await storeJson.merge(effects, {
      jwtSecret: utils.getDefaultString({ charset: 'a-z,A-Z,0-9', len: 64 }),
    })
  }
})
```

## Watch State and Prompt (the admin-credentials pattern)

For state the user owns — admin passwords, API tokens, primary URL — pair a `setupOnInit` watcher with an action. The watcher reads the store and, when the field is unset, surfaces a critical task pointing to the action. The action handles generation, storage, and display, so first-set and later rotation share one code path.

```typescript
// init/watchCredentials.ts
import { setAdminPassword } from '../actions/setAdminPassword'
import { storeJson } from '../fileModels/store.json'
import { i18n } from '../i18n'
import { sdk } from '../sdk'

export const watchCredentials = sdk.setupOnInit(async effects => {
  const store = await storeJson.read().const(effects)

  if (!store?.adminPassword) {
    await sdk.action.createOwnTask(effects, setAdminPassword, 'critical', {
      reason: i18n('Set the admin password before signing in'),
    })
  }
})
```

The matching `setAdminPassword` action lives in `startos/actions/` and looks like:

```typescript
// actions/setAdminPassword.ts
import { utils } from '@start9labs/start-sdk'
import { storeJson } from '../fileModels/store.json'
import { i18n } from '../i18n'
import { sdk } from '../sdk'

export const setAdminPassword = sdk.Action.withoutInput(
  'set-admin-password',
  async () => ({
    name: i18n('Set Admin Password'),
    description: i18n('Generate a new random password for the admin account. Replaces any existing password.'),
    warning: null,
    allowedStatuses: 'any',
    group: null,
    // `'enabled'` keeps the action reachable from the Actions tab so the user
    // can rotate the password later.
    visibility: 'enabled',
  }),
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

If the upstream service needs the password applied via CLI or API rather than just read from the store at startup, wrap the work in `sdk.SubContainer.withTemp()` inside the action handler — see the [Reset a Password](recipe-reset-password.md) recipe.

## Registering a custom init function

Add your custom init function to `init/index.ts`:

```typescript
import { sdk } from '../sdk'
import { setDependencies } from '../dependencies'
import { setInterfaces } from '../interfaces'
import { versionGraph } from '../versions'
import { actions } from '../actions'
import { restoreInit } from '../backups'
import { seedFiles } from './seedFiles'

export const init = sdk.setupInit(
  restoreInit,
  versionGraph,
  setInterfaces,
  setDependencies,
  actions,
  seedFiles, // Add this
)

export const uninit = sdk.setupUninit(versionGraph)
```

## runUntilSuccess Pattern

Use `runUntilSuccess(timeout)` to run daemons and oneshots during init, waiting for completion before continuing. This is essential for setup tasks that need a running server.

### Oneshots Only

For simple sequential tasks (like database migrations):

```typescript
await sdk.Daemons.of(effects)
  .addOneshot('migrate', {
    subcontainer: appSub,
    exec: { command: ['python', 'manage.py', 'migrate', '--noinput'] },
    requires: [],
  })
  .addOneshot('create-superuser', {
    subcontainer: appSub,
    exec: {
      command: ['python', 'manage.py', 'createsuperuser', '--noinput'],
      env: {
        DJANGO_SUPERUSER_USERNAME: 'admin',
        DJANGO_SUPERUSER_PASSWORD: adminPassword,
      },
    },
    requires: ['migrate'],
  })
  .runUntilSuccess(120_000) // 2 minute timeout
```

### Daemon + Dependent Oneshot

For services that require calling an API after the server starts (e.g., bootstrapping via HTTP):

```typescript
await sdk.Daemons.of(effects)
  .addDaemon('server', {
    subcontainer: appSub,
    exec: { command: ['node', 'server.js'] },
    ready: {
      display: null,
      fn: () =>
        sdk.healthCheck.checkPortListening(effects, 8080, {
          successMessage: 'Server ready',
          errorMessage: 'Server not ready',
        }),
    },
    requires: [],
  })
  .addOneshot('bootstrap', {
    subcontainer: appSub,
    exec: {
      command: [
        'node',
        '-e',
        `fetch('http://127.0.0.1:8080/api/bootstrap', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ password: '${adminPassword}' })
        }).then(r => {
          if (!r.ok) throw new Error('Bootstrap failed');
          process.exit(0);
        }).catch(e => {
          console.error(e);
          process.exit(1);
        })`,
      ],
    },
    requires: ['server'], // Waits for daemon to be healthy
  })
  .runUntilSuccess(120_000)
```

**How it works:**

1. The daemon starts and runs its health check
2. Once healthy, the dependent oneshot executes
3. When the oneshot completes successfully, `runUntilSuccess` returns
4. All processes are cleaned up automatically

### Making HTTP Calls Without curl

Many slim Docker images do not have curl. Use the runtime's built-in HTTP capabilities instead.

**Node.js (v18+):**

```typescript
command: [
  'node',
  '-e',
  `fetch('http://127.0.0.1:${port}/api/endpoint', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ key: 'value' })
  }).then(r => r.ok ? process.exit(0) : process.exit(1))
    .catch(() => process.exit(1))`,
]
```

**Python:**

```typescript
command: [
  'python',
  '-c',
  `import urllib.request, json
req = urllib.request.Request(
  'http://127.0.0.1:${port}/api/endpoint',
  data=json.dumps({'key': 'value'}).encode(),
  headers={'Content-Type': 'application/json'},
  method='POST'
)
urllib.request.urlopen(req)`,
]
```

## Reporting Init Progress

Init progress is surfaced in the **Installing** / **Updating** phase of the install, so a long first-run setup (migrations, bootstrapping a server, downloading assets) shows a moving bar instead of an apparent stall. This mirrors backup progress reporting.

You never call the progress effect directly. The init harness builds one `FullProgressTracker` and passes it to **every** init handler as a third argument. Each handler adds its own phases (with its own names) to the shared tracker, unaware of the others. Add phases and update them — **every update auto-reports to the host in the background**, so there's nothing to flush by hand.

`progress.addPhase(name, contribution)` returns a `PhaseHandle` with `start()`, `setTotal(n)`, `setDone(n)`, `setUnits('steps' | 'bytes')`, and `complete()`. Just update the handle; the report follows automatically.

```typescript
export const initializeService = sdk.setupOnInit(async (effects, kind, progress) => {
  if (kind !== 'install') return

  const phase = progress.addPhase('Seeding files', 1)
  phase.setUnits('steps')
  phase.setTotal(seedFiles.length)

  for (let i = 0; i < seedFiles.length; i++) {
    await seedFiles[i](effects)
    phase.setDone(i + 1) // auto-reports in the background
  }

  phase.complete()
})
```

Auto-sync is coalesced — at most one report is in flight and one queued, so a tight update loop collapses to the latest snapshot instead of stacking up calls. If you ever need to guarantee the latest state has landed before doing something else, `await progress.sync()` flushes the in-flight and queued reports (the harness already does this when your handler returns).

> [!NOTE]
> Progress reporting is a no-op outside the install / update / restore transition, so updating phases on a plain container rebuild is harmless. If you need to construct a tracker yourself (rare), it's available as `utils.FullProgressTracker`; no deep import.

### Reporting Progress From a Migration

Migrations receive the same kind of tracker through their opts, so a slow data migration shows progress during an update instead of stalling the bar:

```typescript
// versions/v2_0_0.ts
import { VersionInfo, IMPOSSIBLE } from '@start9labs/start-sdk'
import { i18n } from '../i18n'

export const v2_0_0 = VersionInfo.of({
  version: '2.0.0:0',
  releaseNotes: i18n('Reticulated splines'),
  migrations: {
    up: async ({ effects, progress }) => {
      const records = await loadRecordsToReencode()
      const phase = progress.addPhase('Re-encoding records', 1)
      phase.setUnits('steps')
      phase.setTotal(records.length)

      for (let i = 0; i < records.length; i++) {
        await reencode(records[i])
        phase.setDone(i + 1) // auto-reports in the background
      }

      phase.complete()
    },
    down: IMPOSSIBLE,
  },
})
```

### Multi-phase Handlers

For a handler with several distinct sub-tasks, add one phase per task. The tracker weights them by their `contribution` and reports a combined percentage:

```typescript
export const bootstrap = sdk.setupOnInit(async (effects, kind, progress) => {
  if (kind !== 'install') return

  const dbPhase = progress.addPhase('Initializing database', 1)
  const seedPhase = progress.addPhase('Seeding admin user', 1)

  dbPhase.start()
  await initDatabase(effects)
  dbPhase.complete()

  seedPhase.start()
  await seedAdminUser(effects)
  seedPhase.complete()
})
```

## Common Patterns

### Generate Random Password

```typescript
import { utils } from '@start9labs/start-sdk'

const password = utils.getDefaultString({
  charset: 'a-z,A-Z,0-9',
  len: 22,
})
```

### Create User Task

Prompt the user to run an action — typically when state init detects is missing:

```typescript
await sdk.action.createOwnTask(effects, setAdminPassword, 'critical', {
  reason: i18n('Set the admin password before signing in'),
})
```

Severity levels: `'critical'`, `'important'`, `'optional'`

### Checking Init Kind

```typescript
export const seedFiles = sdk.setupOnInit(async (effects, kind) => {
  // kind === 'install': Fresh install
  // kind === 'update': After version upgrade
  // kind === 'restore': Restoring from backup
  // kind === null: Container rebuild / server restart

  if (kind === 'install') {
    // Generate new passwords, bootstrap server
  }

  if (!kind) return
  // Reached only on install/update/restore — skips container rebuild.

  // No check: runs on ALL init types (install, update, restore, container rebuild)
})
```

> [!TIP]
> `if (!kind) return` is the common guard for "install, update, or restore — but not a plain container rebuild." The inverse (`if (kind) return`) would mean "only on rebuild" — almost never what you want.

### Empty-Seed Inits: Drop the `kind` Parameter

When a `setupOnInit` does nothing but seed file models with their schema defaults (`fileModel.merge(effects, {})`), drop the `kind` parameter entirely — the overhead of running on every init is negligible, and it keeps the logic trivially correct:

```typescript
// init/seedFiles.ts
export const seedFiles = sdk.setupOnInit(async effects => {
  await storeJson.merge(effects, {})
  await configToml.merge(effects, {})
})
```

Reach for the `kind` check only when the body needs to behave differently between install / update / restore / rebuild.

> [!NOTE]
> Always use `merge()` (not `write()`) to seed file models, even on first install. With every key in your zod schema carrying a `.catch()`, `merge(effects, {})` is enough to create the file and fill in every missing default. See [File Models — Prefer merge() Over write()](./file-models.md#prefer-merge-over-write) and [What an Empty merge() Does](./file-models.md#what-an-empty-merge-does).
