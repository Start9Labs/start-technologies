# Main

`setupMain()` defines the runtime behavior of your service -- daemons, health checks, volume mounts, environment variables, and config file generation. It runs each time the service is started.

## Basic Structure

```typescript
import { i18n } from "./i18n";
import { sdk } from "./sdk";
import { uiPort } from "./utils";

export const main = sdk.setupMain(async ({ effects }) => {
  /**
   * ======================== Setup (optional) ========================
   *
   * In this section, we fetch any resources or run any desired preliminary commands.
   */
  console.info(i18n("Starting Hello World!"));

  /**
   * ======================== Daemons ========================
   *
   * In this section, we create one or more daemons that define the service runtime.
   *
   * Each daemon defines its own health check, which can optionally be exposed to the user.
   */
  return sdk.Daemons.of(effects).addDaemon("primary", {
    subcontainer: sdk.SubContainer.of(
      effects,
      { imageId: "hello-world" },
      sdk.Mounts.of().mountVolume({
        volumeId: "main",
        subpath: null,
        mountpoint: "/data",
        readonly: false,
      }),
      "hello-world-sub",
    ),
    exec: { command: ["hello-world"] },
    ready: {
      display: i18n("Web Interface"),
      fn: () =>
        sdk.healthCheck.checkPortListening(effects, uiPort, {
          successMessage: i18n("The web interface is ready"),
          errorMessage: i18n("The web interface is not ready"),
        }),
    },
    requires: [],
  });
});
```

> [!NOTE]
> For a daemon set that changes at runtime — one per tunnel, site, or account — use `sdk.Daemons.dynamic(async ({ effects }) => …)` as your `main` export instead of `setupMain`. It reconciles the running daemons against a freshly-built list on each config change rather than restarting all of them. See [Create Dynamic Daemons](./recipe-dynamic-daemons.md).

## SubContainers

SubContainers are isolated filesystem environments created from Docker images. They provide the rootfs for running daemons, oneshots, and one-off commands.

### Creating SubContainers

**`SubContainer.of()`** -- creates a long-lived subcontainer (for daemons and oneshots):

```typescript
const appSub = sdk.SubContainer.of(
  effects,
  { imageId: "my-app" },
  sdk.Mounts.of().mountVolume({
    volumeId: "main",
    subpath: null,
    mountpoint: "/data",
    readonly: false,
  }),
  "my-app-sub",
);
```

> [!NOTE]
> `SubContainer.of()` is **lazy** — it returns immediately and only materializes the filesystem on first use, so you pass it straight to `addDaemon()` with no `await`. If you need a synchronous `.rootfs`, `.guid`, or `.subpath()` before running anything, `await` the accessor or create it eagerly with `sdk.SubContainer.eager(...)`.

**`SubContainer.withTemp()`** -- creates a temporary subcontainer that is automatically destroyed after the callback completes. Use this for one-off commands in actions, init functions, or migrations:

```typescript
await sdk.SubContainer.withTemp(
  effects,
  { imageId: "my-app" },
  mounts,
  "temp-task",
  async (sub) => {
    await sub.execFail(["my-command", "--flag"]);
  },
);
```

### Image Options

The second argument to `SubContainer.of()` and `SubContainer.withTemp()` accepts:

| Option      | Type    | Default | Description                                                            |
| ----------- | ------- | ------- | ---------------------------------------------------------------------- |
| `imageId`   | string  | —       | Required. The Docker image ID from the manifest `images` field         |
| `sharedRun` | boolean | `false` | Bind-mount the host's `/run` directory into the subcontainer           |

By default, subcontainers share `/dev` and `/sys` with the host. Setting `sharedRun: true` additionally shares `/run`, giving access to host runtime sockets (D-Bus, systemd, PID files). Most services do not need this -- only use it when the container must communicate with host system services.

### Convention: Inline SubContainer.of()

When a subcontainer is only used by one daemon, inline the `SubContainer.of()` call directly inside `addDaemon()` rather than extracting it into a separate variable. Only extract to a variable when the same subcontainer is reused across multiple daemons, oneshots, or exec calls. See the basic example at the top of this page.

## Reactive vs One-time Reads

When reading configuration in `main.ts`, you choose how the system responds to changes:

| Method            | Returns             | Behavior on Change                                  |
| ----------------- | ------------------- | --------------------------------------------------- |
| `.once()`         | Parsed content only | Nothing -- value is stale                           |
| `.const(effects)` | Parsed content      | Re-runs the `setupMain` context, restarting daemons |

```typescript
// Reactive: re-runs setupMain when value changes (restarts daemons)
const store = await storeJson.read().const(effects);

// One-time: read once, no re-run on change
const store = await storeJson.read().once();
```

### Subset Reading

Use a mapper function to read only specific fields. This is more efficient and limits reactivity to only the fields you care about:

```typescript
// Read only secretKey - re-runs only if secretKey changes
const secretKey = await storeJson.read((s) => s.secretKey).const(effects);
```

### Other Reading Methods

| Method                         | Purpose                             |
| ------------------------------ | ----------------------------------- |
| `.onChange(effects, callback)` | Register callback for value changes |
| `.watch(effects)`              | Create async iterator of new values |

## Getting Hostnames

Interfaces are reached through their **host**. `sdk.host.getOwn(effects, hostId)` returns the host (`hostId` is the id you passed to `sdk.MultiHost.of`); the interface you exported lives under one of the host's bindings, and its `addressInfo` comes back **pre-filled** — call `.format(...)` on it for resolvable hostnames/URLs (also `.filter(...)`, `.nonLocal`, `.public`, `.bridge`, `.toUrl`):

```typescript
const host = await sdk.host.getOwn(effects, "ui").const();
const ui = Object.values(host?.bindings ?? {})
  .flatMap((b) => Object.values(b.interfaces))
  .find((i) => i.id === "ui");

const allowedHosts =
  ui?.addressInfo.format("hostname-info").map((h) => h.hostname.value) ?? [];
```

`.const()` sets up a reactive watcher — `setupMain` re-runs whenever the host's bindings, addresses, or exported interfaces change.

To react to only a slice of the host, pass a `map` selector (and optional `eq`, default deep-equal) to `getOwn`/`get`. `.const()` then re-runs only when the mapped value changes rather than on any change to the whole host:

```typescript
// re-run only when THIS interface's address info changes
const ui = await sdk.host
  .getOwn(effects, "ui", (host) => host?.bindings[80]?.interfaces["ui"])
  .const();
```

## Oneshots (Runtime)

Oneshots are tasks that run on every startup before daemons. Use them for idempotent operations like migrations:

```typescript
// change ownership of a directory
.addOneshot('chown', {
  subcontainer,
  exec: {
    command: ['chown', '-R', 'user:user', '/data',],
    user: 'root',
  },
  requires: [],
})
.addOneshot('collectstatic', {
  subcontainer: appSub,
  exec: { command: ['python', 'manage.py', 'collectstatic', '--noinput'] },
  requires: ['migrate'],
})
```

> [!WARNING]
> Do NOT put one-time setup tasks (like `createsuperuser`) in `main.ts` oneshots -- they run on every startup and will fail on subsequent runs. Use a custom init file (e.g. `init/seedFiles.ts`) instead. See [Initialization Patterns](./init.md) for details.

## Exec Command

### Using Upstream Entrypoint

If the upstream Docker image has a compatible `ENTRYPOINT`/`CMD`, use `sdk.useEntrypoint()` instead of specifying a custom command. This is the simplest approach and ensures compatibility with the upstream image:

```typescript
.addDaemon('primary', {
  subcontainer: appSub,
  exec: {
    command: sdk.useEntrypoint(),
  },
  // ...
})
```

You can pass an array of arguments to override the image's `CMD` while keeping the `ENTRYPOINT`:

```typescript
.addDaemon('postgres', {
  subcontainer: postgresSub,
  exec: {
    command: sdk.useEntrypoint(['-c', 'listen_addresses=127.0.0.1']),
  },
  // ...
})
```

**When to use `sdk.useEntrypoint()`:**

- Upstream image has a working entrypoint that starts the service correctly
- You want to use the entrypoint but optionally override CMD arguments
- Examples: Ollama, Jellyfin, Vaultwarden, Postgres

### Custom Command

Use a custom command array when you need to bypass the entrypoint entirely:

```typescript
.addDaemon('primary', {
  subcontainer: appSub,
  exec: {
    command: ['/opt/app/bin/start.sh', '--port=' + uiPort],
  },
  // ...
})
```

### Running the Entrypoint as PID 1 (`runAsInit`)

Some images bundle their own init system or process supervisor — `s6-overlay` (used by **every** `linuxserver/*` image), `tini`, `dumb-init`, or `supervisord` — and that supervisor expects to run as **PID 1**. In a StartOS subcontainer the daemon command is not PID 1 by default, so such a supervisor aborts on startup (s6 logs `s6-overlay-suexec: fatal: can only run as pid 1`). Set `runAsInit: true` on the `exec` to make the command the container's init process:

```typescript
.addDaemon('primary', {
  subcontainer: appSub,
  exec: {
    command: sdk.useEntrypoint(),
    runAsInit: true, // image bundles s6-overlay / tini / supervisord, which must be PID 1
    env: { PUID: '1000', PGID: '1000', TZ: 'Etc/UTC' },
  },
  // ...
})
```

**When to use `runAsInit: true`:**

- The image uses `s6-overlay` (any `linuxserver/*` image), `tini`, `dumb-init`, or `supervisord` as its entrypoint
- The daemon starts but its supervisor immediately crashes complaining it is not PID 1

Leave it off (the default) for images whose entrypoint is the application binary itself. (`runAsInit` is declared on the `exec` options in `Daemons.d.ts` — like many SDK options, it's easier to find by grepping the types than by searching the docs; see [Search the SDK before deciding something is impossible](workflow.md#search-the-sdk-before-deciding-something-is-impossible).) See [Package a Prebuilt Docker Image](recipe-prebuilt-image.md) for the full prebuilt-image workflow.

## Environment Variables

Pass environment variables to a daemon or oneshot via the `env` option on `exec`:

```typescript
.addDaemon('main', {
  subcontainer: appSub,
  exec: {
    command: sdk.useEntrypoint(),
    env: {
      DATABASE_URL: 'sqlite:///data/db.sqlite3',
      SECRET_KEY: store?.secretKey ?? '',
    },
  },
  // ...
})
```

## Health Checks

There are two kinds of health checks:

### Daemon Readiness (`ready`)

Every daemon has a `ready` property that tells StartOS when the daemon has started. This is defined inline on the daemon and determines when dependent daemons (via `requires`) can start:

```typescript
.addDaemon('app', {
  subcontainer: appSub,
  exec: { command: sdk.useEntrypoint() },
  ready: {
    display: i18n('Web Interface'),
    fn: () =>
      sdk.healthCheck.checkPortListening(effects, 8080, {
        successMessage: i18n('Ready'),
        errorMessage: i18n('Starting...'),
      }),
    gracePeriod: 30_000,  // optional: treat failures as "starting" for this long (ms)
  },
  requires: [],
})
```

Use `display: null` for internal daemons (databases, caches) whose readiness check should not be shown to the user.

### Standalone Health Checks (`addHealthCheck`)

For ongoing conditions beyond daemon readiness — sync progress, network reachability, secondary interface availability — use `.addHealthCheck()` in the daemon chain. These run continuously and are displayed to the user. Their IDs are what dependency packages reference in their `healthChecks` array.

```typescript
.addHealthCheck('sync-progress', {
  ready: {
    display: i18n('Sync Progress'),
    fn: async () => {
      const res = await appSub.exec(['myapp', 'sync-status'])
      const synced = res.exitCode === 0
      return {
        result: synced ? 'success' : 'loading',
        message: synced ? 'Fully synced' : 'Syncing...',
      }
    },
  },
  requires: ['app'],  // only runs after 'app' daemon is ready
})
```

A health check can also return `result: 'disabled'` with an informational message when the check does not apply (e.g., reachability check when no public address is configured).

Standalone health checks can be conditional — return `null` instead of the config object to skip the check entirely:

```typescript
.addHealthCheck('optional-feature', () =>
  featureEnabled
    ? { ready: { display: i18n('Feature'), fn: checkFn }, requires: ['app'] }
    : null,
)
```

### Health Check Result States

The `fn` returns an object with `result` and `message`:

| Result | Meaning | When to use |
|--------|---------|-------------|
| `success` | Healthy and fully operational | Service is ready and serving |
| `loading` | Operational but catching up | Syncing blocks, indexing data |
| `disabled` | Intentionally inactive | Feature excluded by config (e.g. onlynet) |
| `starting` | Not yet ready | Still initializing (also set automatically during `gracePeriod`) |
| `failure` | Unhealthy | Process crashed, port not listening, dependency unreachable |

`loading` and `failure` require a `message` string. Other states accept an optional message.

### Built-in Health Check Helpers

Available on `sdk.healthCheck`:

- **`checkPortListening(effects, port, { successMessage, errorMessage })`** — checks if a TCP/UDP port is bound by reading `/proc/net`. Lightweight, no network I/O. Preferred for daemon readiness checks.
- **`checkWebUrl(effects, url, { successMessage, errorMessage })`** — fetches a URL, succeeds on any HTTP response.
- **`runHealthScript(command, subcontainer, { errorMessage })`** — runs a command in a subcontainer, succeeds on exit code 0.

### Polling Triggers

By default, health checks poll every 1 s while the daemon is pending, then every 30 s once it reports a non-pending result (`success`, `loading`, or `disabled`). Override this with the `trigger` option on `ready`:

```typescript
ready: {
  display: i18n('Sync Progress'),
  trigger: sdk.trigger.cooldownTrigger(30_000),  // fixed 30s interval
  fn: async () => { /* ... */ },
}
```

Available triggers on `sdk.trigger`:

- **`cooldownTrigger(ms)`** — fixed interval between checks, regardless of status.
- **`statusTrigger(defaultMs, { success?, loading?, disabled?, starting?, waiting?, failure? })`** — per-status polling intervals in milliseconds. The first argument is the default interval for any status not explicitly listed.

Use a slower trigger for expensive checks (RPC calls during heavy processing) to reduce load on the service:

```typescript
trigger: sdk.trigger.statusTrigger(30_000, {
  starting: 5_000,
  failure: 5_000,
}),
```

## Volume Mounts

```typescript
sdk.Mounts.of()
  // Mount entire volume (directory)
  .mountVolume({
    volumeId: "main",
    subpath: null,
    mountpoint: "/data",
    readonly: false,
  })
  // Mount specific file from volume (requires type: 'file')
  .mountVolume({
    volumeId: "main",
    subpath: "config.py",
    mountpoint: "/app/config.py",
    readonly: true,
    type: "file", // Required when mounting a single file
  });
```

> [!WARNING]
> `sdk.Mounts` is an immutable builder. Every `mountVolume` / `mountAssets` / `mountDependency` / `mountBackups` call returns a **new** `Mounts` instance — the original is unchanged. Discarded return values silently drop the mount.
>
> ```typescript
> // BROKEN — conditional mount is lost
> const mounts = sdk.Mounts.of().mountVolume({ /* ... */ });
> if (needsCookie) {
>   mounts.mountDependency({ /* ... */ }); // ← return value discarded
> }
>
> // CORRECT — reassign each time
> let mounts = sdk.Mounts.of().mountVolume({ /* ... */ });
> if (needsCookie) {
>   mounts = mounts.mountDependency({ /* ... */ });
> }
> ```
>
> Chained calls (`.mountVolume(...).mountDependency(...)`) are fine — the returned instance flows into the next call. The trap is conditional mutation with the return thrown away. Symptom: the file you expected at the mountpoint isn't there, so a `FileHelper.string(...).read()` returns `null` or a subcontainer read fails.

### Remapping Ownership (`idmap`)

Every mount (`mountVolume` / `mountAssets` / `mountDependency` / `mountBackups`) takes an optional **`idmap`** — a list of `{ fromId, toId, range? }` entries that remap ownership at the mount boundary, so files stored under one uid/gid on the volume appear under the uid/gid the service expects. `fromId` is the id seen on the filesystem, `toId` is the id processes in the container see, and `range` (default `1`) covers that many consecutive ids. The container's own LXC id-mapping is applied automatically — don't include it here.

```typescript
sdk.Mounts.of().mountVolume({
  volumeId: "main",
  subpath: null,
  mountpoint: "/data",
  readonly: false,
  idmap: [{ fromId: 0, toId: 1000 }], // files owned by uid 0 on the volume appear as uid 1000 in the container
});
```

## Writing to Subcontainer Rootfs

For config files that are *generated from code* on every startup (e.g., a Python settings file built from hostnames and secrets), write directly to the subcontainer's rootfs:

```typescript
import { writeFile } from 'node:fs/promises'

// Write a generated config to subcontainer rootfs
await writeFile(
  `${await appSub.rootfs}/app/config.py`,
  generateConfig({ secretKey, allowedHosts }),
)
```

> [!WARNING]
> If the config file is managed by a FileModel, do NOT read it and write it back to rootfs. Mount it from the volume instead — the file already exists there.

**When to use rootfs vs volume mounts:**

- **Rootfs**: Config files *generated from code* that don't exist on a volume (e.g., built from hostnames, env vars, or templates)
- **Volume mount (directory)**: Mount a directory that contains the config file alongside other persistent data. The config file is just one of many files in the mounted directory.
- **Volume mount (file)**: Mount a single config file with `type: 'file'` when the config lives on a volume that is otherwise unrelated to the container's filesystem.

## Executing Commands in SubContainers

Use `exec` or `execFail` to run commands in a subcontainer:

| Method       | Behavior on Non-zero Exit                                            |
| ------------ | -------------------------------------------------------------------- |
| `exec()`     | Returns result with `exitCode`, `stdout`, `stderr` -- does NOT throw |
| `execFail()` | Throws an error on non-zero exit code                                |

```typescript
// exec() - manual error handling (good for optional/warning cases)
const result = await appSub.exec(["update-ca-certificates"], { user: "root" });
if (result.exitCode !== 0) {
  console.warn("Failed to update CA certificates:", result.stderr);
}

// execFail() - throws on error (good for required commands)
// Uses the default user from the Dockerfile (no need to specify { user: '...' })
await appSub.execFail(["git", "clone", "https://github.com/user/repo.git"]);

// Override user when needed (e.g., run as root)
await appSub.exec(["update-ca-certificates"], { user: "root" });
```

The `user` option is optional. If omitted, commands run as the default user defined in the Dockerfile (`USER` directive). Only specify `{ user: 'root' }` when you need elevated privileges.

**Use `execFail()` when:**

- The command must succeed for the service to work correctly
- You are in a custom init file (e.g. `seedFiles.ts`) and want installation to fail if setup fails
- You want automatic error propagation

**Use `exec()` when:**

- The command failure is not critical (warnings, optional setup)
- You need to inspect the exit code or output regardless of success/failure
- You want custom error handling logic

## PostgreSQL Sidecar

Many services require a PostgreSQL database. Run it as a sidecar daemon within the same service package.

### Security Model

Use password authentication with localhost-only binding. Auto-generate the password on install and store it in your `store.json` FileModel.

**Password generation** (in `utils.ts`):

```typescript
import { utils } from "@start9labs/start-sdk";

export function getDefaultPgPassword(): string {
  return utils.getDefaultString({ charset: "a-z,A-Z,0-9", len: 22 });
}
```

**Store schema** (in `fileModels/store.json.ts`):

```typescript
const shape = z.object({
  pgPassword: z.string().catch(""),
  // ...other fields
});
```

**Seed on install** (in `init/seedFiles.ts`):

```typescript
export const seedFiles = sdk.setupOnInit(async (effects, kind) => {
  if (kind !== "install") return;
  await storeJson.merge(effects, {
    pgPassword: getDefaultPgPassword(),
  });
});
```

**Seed on upgrade** (in version migration):

```typescript
// Generate pgPassword for users upgrading from a version that didn't have one
const existing = await storeJson.read((s) => s.pgPassword).once();
await storeJson.merge(effects, {
  pgPassword: existing || getDefaultPgPassword(),
});
```

### Daemon Configuration

```typescript
import { sdk } from "./sdk";
import { i18n } from "./i18n";

// Read password from store
const pgPassword = store.pgPassword;

// Define mounts for PostgreSQL data
const pgMounts = sdk.Mounts.of().mountVolume({
  volumeId: "main",
  subpath: "postgresql",
  mountpoint: "/var/lib/postgresql",
  readonly: false,
});

// Create subcontainer
const postgresSub = sdk.SubContainer.of(
  effects,
  { imageId: "postgres" },
  pgMounts,
  "postgres",
);

// Add as daemon
.addDaemon('postgres', {
  subcontainer: postgresSub,
  exec: {
    command: sdk.useEntrypoint(['-c', 'listen_addresses=127.0.0.1']),
    env: {
      POSTGRES_PASSWORD: pgPassword,
    },
  },
  ready: {
    display: null, // Internal service, not shown in UI
    fn: async () => {
      const result = await postgresSub.exec([
        'pg_isready', '-q', '-h', '127.0.0.1',
        '-d', 'postgres', '-U', 'postgres',
      ])
      if (result.exitCode !== 0) {
        return {
          result: 'loading',
          message: i18n('Waiting for PostgreSQL to be ready'),
        }
      }
      return {
        result: 'success',
        message: i18n('PostgreSQL is ready'),
      }
    },
  },
  requires: [],
})
```

Key points:
- **`listen_addresses=127.0.0.1`**: Restricts connections to localhost only — no external access
- **`POSTGRES_PASSWORD`**: Auto-generated password, stored in `store.json`
- **`display: null`**: Internal sidecar health checks are typically not shown to the user

### Connection Strings

When the upstream service needs a PostgreSQL connection string, include the password:

```typescript
.addDaemon('app', {
  subcontainer: appSub,
  exec: {
    command: sdk.useEntrypoint(),
    env: {
      // Standard PostgreSQL URI
      DATABASE_URL: `postgresql://postgres:${pgPassword}@127.0.0.1:5432/mydb`,
      // Or .NET-style: `User ID=postgres;Password=${pgPassword};Host=127.0.0.1;Port=5432;Database=mydb`
    },
  },
  requires: ['postgres'],
})
```

> [!NOTE]
> The Docker entrypoint for the official `postgres` image handles initial database creation automatically. You do not need to run `createdb` or `initdb` manually on fresh installs.

### Querying PostgreSQL from Actions

Some actions need to query PostgreSQL directly (e.g., resetting a user password). Read the password from the store:

```typescript
import { Client } from 'pg'

const pgPassword = (await storeJson.read((s) => s.pgPassword).once()) || ''

const client = new Client({
  user: 'postgres',
  password: pgPassword,
  host: '127.0.0.1',
  database: 'mydb',
  port: 5432,
})

try {
  await client.connect()
  await client.query(
    `UPDATE "Users" SET "PasswordHash"=$1 WHERE "Id"=$2`,
    [hash, userId],
  )
} finally {
  await client.end()
}
```

> [!WARNING]
> When interpolating values into raw SQL strings (e.g., for `psql -c`), always escape single quotes to prevent SQL injection:
>
> ```typescript
> function sqlLiteral(value: string): string {
>   return `'${value.replace(/'/g, "''")}'`
> }
>
> // Use in psql commands
> await sub.execFail(
>   ['psql', '-c', `ALTER USER myuser PASSWORD ${sqlLiteral(password)}`],
>   { user: 'postgres' },
> )
> ```
>
> Prefer parameterized queries (the `$1` syntax above) whenever possible — they handle escaping automatically.

## Config File Generation

A common pattern is to define a helper function that generates a config file string from your service's configuration values:

```typescript
function generateConfig(config: {
  secretKey: string;
  allowedHosts: string[];
}): string {
  const hostsList = config.allowedHosts.map((h) => `'${h}'`).join(", ");

  return `
SECRET_KEY = '${config.secretKey}'
ALLOWED_HOSTS = [${hostsList}]
DATABASE = '/data/db.sqlite3'
`;
}
```
