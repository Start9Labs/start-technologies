# File Models

File Models represent configuration files as TypeScript definitions using [zod](https://zod.dev/) schemas. They provide type safety, runtime validation, and automatic enforcement of defaults and hardcoded values throughout your codebase.

## Supported Formats

File Models support automatic parsing and serialization for:

- `.json`
- `.yaml` / `.yml`
- `.toml`
- `.xml`
- `.ini`
- `.env`

Custom parser/serializer support is available for non-standard formats via `FileHelper.raw()`.

## Core Principle: Lean on File Models

File models are not just type definitions — they are your **primary tool for enforcing runtime correctness**. The zod schema is both the shape definition _and_ the source of truth for default values. Every key should have a `.catch()` so that:

- Missing keys are filled with defaults automatically
- Invalid values are corrected on the next `merge()`
- Files can be seeded with `merge(effects, {})` on first install — no separate default object needed
- Hardcoded values (ports, paths, auth modes) are enforced on every read

When done correctly, the shape itself eliminates the need for separate default constants, defensive checks, and manual file initialization.

## Creating a File Model

### store.json.ts (Common Pattern)

The most common file model is `store.json`, used to persist internal service state:

```typescript
import { FileHelper, z } from '@start9labs/start-sdk'
import { sdk } from '../sdk'

const shape = z.object({
  adminPassword: z.string().optional().catch(undefined),
  secretKey: z.string().optional().catch(undefined),
  someNumber: z.number().catch(0),
  someFlag: z.boolean().catch(false),
})

export const storeJson = FileHelper.json({ base: sdk.volumes.main, subpath: './store.json' }, shape)
```

### YAML Configuration

```typescript
import { FileHelper, z } from '@start9labs/start-sdk'
import { sdk } from '../sdk'

const serverSchema = z.object({
  host: z.string().catch('localhost'),
  port: z.number().catch(8080),
})

const shape = z.object({
  server: serverSchema.catch(() => serverSchema.parse({})),
  features: z.array(z.string()).catch([]),
})

export const configYaml = FileHelper.yaml({ base: sdk.volumes.main, subpath: 'config.yaml' }, shape)
```

### TOML Configuration

```typescript
import { FileHelper, z } from '@start9labs/start-sdk'
import { sdk } from '../sdk'

const shape = z.object({
  api_bind: z.literal('0.0.0.0').catch('0.0.0.0'),
  api_port: z.literal(9814).catch(9814),
  debug: z.literal(false).catch(false),
  subscription_slots: z.literal(10_000).catch(10_000),
})

export const configToml = FileHelper.toml({ base: sdk.volumes.main, subpath: 'config.toml' }, shape)
```

### XML Configuration

XML support includes options for controlling array detection during parsing:

```typescript
import { FileHelper, z } from '@start9labs/start-sdk'
import { sdk } from '../sdk'

const knownProxiesSchema = z.object({
  // 10.0.3.1 is the OS bridge gateway — the reverse proxy this container should
  // trust. It is the OS's own fixed address (see Service-to-Service Networking),
  // not a dependency dial, so the literal is correct here.
  string: z.literal('10.0.3.1').array().catch(['10.0.3.1']),
})

const networkConfigSchema = z.object({
  KnownProxies: knownProxiesSchema.catch(() => knownProxiesSchema.parse({})),
})

const shape = z.object({
  NetworkConfiguration: networkConfigSchema.catch(() => networkConfigSchema.parse({})),
})

export const networkXml = FileHelper.xml({ base: sdk.volumes.config, subpath: 'network.xml' }, shape, {
  parser: {
    // Tell the XML parser which element names should always be treated as arrays
    isArray: name => name === 'string',
  },
})
```

## Reading File Models

### Reading Methods

| Method                         | Purpose                                                |
| ------------------------------ | ------------------------------------------------------ |
| `.once()`                      | Read once, no reactivity                               |
| `.const(effects)`              | Read and re-run the enclosing context if value changes |
| `.onChange(effects, callback)` | Register a callback for value changes                  |
| `.watch(effects)`              | Create an async iterator of new values                 |
| `.waitFor(effects, predicate)` | Block until the value satisfies a predicate            |

> [!NOTE]
> All read methods return `null` if the file doesn't exist. Do NOT use try-catch for missing files.

### Use the Map Function

When reading file models, **always use the map function** to extract only the fields you need. This is critical for two reasons:

1. **Avoids unnecessary restarts**: With `.const(effects)`, the daemon only restarts when the _mapped_ value changes, not when _any_ field in the file changes.
2. **Avoids unnecessary callbacks**: With `.onChange(effects)` or `.watch(effects)`, your callback only fires when the specific field you care about changes.

```typescript
// BAD: daemon restarts when ANY field changes, even unrelated ones
const store = await storeJson.read().const(effects)
const secretKey = store?.secretKey

// GOOD: daemon only restarts when secretKey changes
const secretKey = await storeJson.read(s => s.secretKey).const(effects)
```

> [!WARNING]
> Never use an identity mapper like `.read((s) => s)`. Either omit the mapper to get the full object (`.read()`) or use it to extract a specific field (`.read((s) => s.someField)`).

### Examples

```typescript
// One-time read (no restart on change) - returns null if file doesn't exist
const store = await storeJson.read().once()

// Handle missing file with nullish coalescing
const keys = (await authorizedKeysFile.read().once()) ?? []

// Reactive read of a specific field - daemon only restarts if secretKey changes
const secretKey = await storeJson.read(s => s.secretKey).const(effects)

// Read nested values
const serverHost = await configYaml.read(c => c.server.host).once()

// Wait until a condition is met (blocks until predicate returns true)
const syncedStore = await storeJson.read(s => s.fullySynced).waitFor(effects, synced => synced === true)
```

## Writing File Models

### Prefer `merge()` Over `write()`

Use `merge()` for almost all writes. It has two major advantages:

1. **Preserves unknown keys**: `merge()` only updates the fields you specify, leaving everything else intact — including keys that the upstream service uses but your file model doesn't define. `write()` replaces the entire file, destroying any keys not in your schema. See [Unknown Key Preservation](#unknown-key-preservation) for details and migration implications.
2. **Defaults come from the schema**: When every key in your zod schema has a `.catch()`, the schema _is_ the default. You can seed a file on first install with `merge(effects, {})` — the `.catch()` values fill in every missing field. No need to define a separate defaults object and pass it to `write()`.

```typescript
// Seed a file on first install — .catch() defaults fill everything in
await configToml.merge(effects, {})

// Update specific fields, preserve everything else
await storeJson.merge(effects, { someFlag: false })

// Update nested fields
await configYaml.merge(effects, { server: { port: 9090 } })
```

Only use `write()` when you intentionally want to replace the entire file — for example, when generating a file from scratch during a migration:

```typescript
// write() replaces the entire file — use only when that's the intent
await storeJson.write(effects, {
  adminPassword: generatedPassword,
  secretKey: generatedKey,
  smtp: { selection: 'disabled', value: {} },
})
```

### Exporting Defaults from File Models

When a default value from the file model is also needed elsewhere (e.g., as a placeholder or default in an action's input spec), define the value as a constant in the file model, use it in the schema, and export it:

```typescript
// fileModels/config.toml.ts
import { FileHelper, z } from '@start9labs/start-sdk'
import { sdk } from '../sdk'

export const defaultMaxUpload = '50M'

const shape = z.object({
  max_upload_size: z.string().catch(defaultMaxUpload),
  allow_registration: z.boolean().catch(false),
})

export const configToml = FileHelper.toml({ base: sdk.volumes.main, subpath: 'config.toml' }, shape)
```

```typescript
// actions/config.ts
import { defaultMaxUpload } from '../fileModels/config.toml'

const inputSpec = InputSpec.of({
  max_upload_size: Value.text({
    name: i18n('Max Upload Size'),
    default: defaultMaxUpload,
    // ...
  }),
})
```

This keeps the default defined in exactly one place.

## Schema Design

### Every Key Should Have `.catch()`

Give every key a `.catch()` default. This makes your file model self-healing — invalid or missing values are automatically corrected, and `merge(effects, {})` works for initialization.

```typescript
const shape = z.object({
  host: z.string().catch('localhost'),
  port: z.number().catch(8080),
  debug: z.boolean().catch(false),
  tags: z.array(z.string()).catch([]),
  apiKey: z.string().optional().catch(undefined),
})
```

### Nested Objects Must Also Have `.catch()`

`.catch()` does **not** cascade to child objects. When a parent key is missing entirely (e.g., parsing `{}`), validation fails at the parent level _before_ any inner defaults can apply.

**The problem:**

```typescript
// BROKEN: inner .catch() values never fire when "server" is missing
const shape = z.object({
  server: z.object({
    host: z.string().catch('localhost'),
    port: z.number().catch(8080),
  }),
})

shape.parse({})
// => ZodError: "server" expected object, received undefined
```

**The fix:** Extract child schemas into variables and use `.catch(() => childSchema.parse({}))`:

```typescript
const serverSchema = z.object({
  host: z.string().catch('localhost'),
  port: z.number().catch(8080),
})

const shape = z.object({
  server: serverSchema.catch(() => serverSchema.parse({})),
})

shape.parse({})
// => { server: { host: 'localhost', port: 8080 } }
```

The `.catch()` callback delegates back to the child schema, so defaults are defined in exactly one place. Extracting child schemas into variables keeps the code DRY — the shape and its defaults are the same thing.

> [!NOTE]
> This pattern only works when **all** inner fields have `.catch()` defaults. If a nested object has required fields without defaults (e.g., a password that must be generated at init time), seed the file with complete data using `write()` instead of relying on `merge(effects, {})`.

### Deep Nesting

When a schema has multiple levels of nesting, extract each level into its own variable. This keeps the top-level shape readable and ensures `.catch()` works at every depth:

```typescript
import { FileHelper, z } from '@start9labs/start-sdk'
import { sdk } from '../sdk'

// Level 2: nested object
const dbDefault = { path: '/data/app.db', journal_mode: 'wal' }
const dbShape = z
  .object({
    path: z.literal('/data/app.db').catch(dbDefault.path),
    journal_mode: z.string().catch(dbDefault.journal_mode),
  })
  .catch(dbDefault)

// Level 2: array item
const endpointDefault = { port: 8080, tls: false }
const endpointShape = z
  .object({
    port: z.number().catch(endpointDefault.port),
    tls: z.boolean().catch(endpointDefault.tls),
  })
  .catch(endpointDefault)

// Top level
const shape = z.object({
  database: dbShape,
  endpoints: z.array(endpointShape).catch([endpointDefault]),
  log_level: z.string().catch('info'),
  max_upload_size: z.string().catch('50M'),
})

export const configYaml = FileHelper.yaml({ base: sdk.volumes.main, subpath: 'config.yaml' }, shape)
```

The key technique: define each nested level's default and shape separately, then compose them. Every level has its own `.catch()` so missing or malformed data at any depth resolves to sane defaults.

### Hardcoded Literal Values

For values that should always be a specific literal and never change (e.g., internal ports, paths, auth modes), use `z.literal().catch()`. If the file ends up with a different value (e.g., user edits it manually), it is corrected on the next `merge()`:

```typescript
const shape = z.object({
  // Enforced — always corrected back to these values
  api_bind: z.literal('0.0.0.0').catch('0.0.0.0'),
  api_port: z.literal(9814).catch(9814),
  btc_network: z.literal('mainnet').catch('mainnet'),
  debug: z.literal(false).catch(false),

  // Mutable — can be changed by actions
  subscription_slots: z.number().catch(10_000),
})
```

This pattern is especially useful for upstream config files where you need to lock down certain values while still letting the user configure others through actions.

### Reparse `raw` Through Shape in `formToFile`

When a `FileHelper.ini` uses an `InputSpec`'s `partialValidator` as its validator and exposes the raw file as `raw: Value.hidden(shape)`, `formToFile` must reparse `rawInput` through `shape` before spreading it. Otherwise, the first install seed writes an empty file — the enforced `.catch()` defaults in `shape` never fire, and the daemon starts with upstream defaults instead of the locked-down values.

```typescript
export const shape = z.object({
  'rpc-bind-ip': z.literal('0.0.0.0').catch('0.0.0.0'),
  'rpc-bind-port': z.literal(18081).catch(18081),
  // ...more enforced + configurable keys
})

export const fullConfigSpec = InputSpec.of({
  raw: Value.hidden(shape),
  // ...user-facing form fields
})

function formToFile(input: T.DeepPartial<typeof fullConfigSpec._TYPE>): Conf {
  const { raw: rawInput, ...rest } = input

  // Reparse through shape so .catch() defaults fire when rawInput is undefined.
  const raw = shape.parse(rawInput ?? {})

  return {
    ...raw,
    // ...form-derived fields
  }
}

export const confFile = FileHelper.ini(
  { base: sdk.volumes.main, subpath: 'my.conf' },
  fullConfigSpec.partialValidator,
  { bracketedArray: false },
  {
    onRead: a => fileToForm(shape.parse(a)),
    onWrite: a => formToFile(a),
  },
)
```

**Why it matters:** `partialValidator` makes every field of `fullConfigSpec` optional — including `raw`. On first install (`confFile.merge(effects, {})` from `seedFiles`), `rawInput` arrives `undefined`, so `...raw` spreads nothing. zod's `.catch()` defaults only fire under `shape.parse()`. Calling `shape.parse(rawInput ?? {})` is what forces them. On subsequent writes, `onRead` has already produced a fully populated conf, so the reparse is idempotent.

**Alternative:** Some packages (`bitcoin-core`, `cln`) hardcode the enforced values in both `shape` (`z.literal(X).catch(X)`) _and_ again inside `formToFile`. That works but duplicates the source of truth — two places to update if a value changes. The reparse keeps `shape` as the single source.

### Unknown Key Preservation

The SDK patches `z.object()` to use loose mode by default — unknown keys in the parsed data are **preserved**, not stripped. This is intentional: upstream config files often contain keys your schema doesn't model (auto-generated secrets, internal state, plugin settings, etc.), and stripping them would break the service.

This has two important consequences:

1. **`merge()` never removes keys you don't mention.** Only keys explicitly passed to `merge()` are updated. Everything else — including keys outside your schema — passes through untouched.
2. **Stale keys from previous versions persist.** If an earlier version of your package wrote keys that the current version no longer uses, those keys survive across updates. They are not automatically cleaned up by `merge()` or by the zod schema.

To **delete a stale key**, pass it as `undefined` in a `merge()` call:

```typescript
// Remove keys that no longer exist in the current version
await configToml.merge(effects, {
  old_deprecated_key: undefined,
  removed_plugin_setting: undefined,
})
```

When stale keys are outside your schema's type, cast the merge data:

```typescript
await configToml.merge(effects, {
  legacy_key: undefined,
} as any)
```

> [!WARNING]
> This removes a stale key your schema doesn't model. It cannot surgically delete one entry of a _typed_ collection that has a `.catch()` default. `merge({ users: { bob: undefined } })` against `users: z.record(...).catch({})` makes the whole `users` value fail validation, so the `.catch({})` replaces the **entire** record with `{}` — every entry is wiped, not just `bob`. To drop one entry while keeping the rest, rebuild the value in code and `write()` it.

### Arrays Are Replaced, Not Merged

`merge()` recurses into plain **objects** key by key, but it treats **arrays and primitives as atomic values** — whatever you pass replaces what was there. For an array this means no element-wise union, append, or de-duplication: the array in your patch becomes the new value in its entirety.

```typescript
// stored: { friends: ["alice", "bob"] }
await storeJson.merge(effects, { friends: ['alice'] })
// result: { friends: ["alice"] }  — "bob" is dropped, not preserved
```

This produces an asymmetry that is easy to get wrong, because object keys and array elements behave oppositely under the same `merge()` call:

| What you merge                | What happens to what you left out                                                                                                 |
| ----------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| An **object** without a key   | The key is **kept** — `merge()` never deletes a key you don't mention (see [Unknown Key Preservation](#unknown-key-preservation)) |
| An **array** without an entry | The entry is **gone** — the whole array is overwritten                                                                            |

So to change one entry of an array, read the current array, edit it in code, and merge the **complete** new array — you cannot add or drop a single element by passing a one-element patch. When you instead need to remove an object key (or otherwise rebuild a structure wholesale), reach for `write()`, which replaces the entire file.

### Using SDK-Provided Schemas

For complex types like SMTP, use the SDK's built-in zod schemas. See [Actions](./actions.md) for the full SMTP configuration walkthrough.

```typescript
import { smtpShape, z } from '@start9labs/start-sdk'

const shape = z.object({
  adminPassword: z.string().optional().catch(undefined),
  smtp: smtpShape,
})
```

### Don't Call `.strip()` on Your Shape

The SDK intentionally patches `z.object()` to loose mode (see [Unknown Key Preservation](#unknown-key-preservation)) so unknown keys from the upstream service survive. Calling `.strip()` on your shape disables that protection and will silently destroy user data on the next `merge()` — keys outside your schema get discarded. Leave the default alone; only use `.strict()` if you have a specific reason to reject unknowns.

## Migration Gotchas

### Parser / Separator Transitions Can Wipe Data

If you change a FileHelper's parser or separator on an already-released package — e.g. switching from `FileHelper.ini` (npm `ini`, `=` separator) to `FileHelper.raw` with a custom parser that uses `: ` — existing on-disk files may silently decay under the new code. The old format isn't recognized, every section parses as `{}`, zod `.catch()` defaults fill in, and the defaulted object is stringified back in the new format. Real user data (passwords, custom settings) gets quietly replaced with defaults.

`.catch()` defaults are great for new installs but mask exactly this class of error — there is no parse failure to observe.

Before shipping a parser change:

- Verify the new parser actually reads what the **old** code and the upstream service wrote. If not, plan a one-shot migration that rewrites the file in the new format as part of the version upgrade.
- When diagnosing a "field silently became empty / default" bug after an update, check git history for parser, separator, or FileHelper implementation changes on the affected file model.

## Design Guidelines

### Prefer Direct FileModel Over store.json + Environment Variables

When an upstream service reads a config file (TOML, YAML, JSON, XML, etc.), model that file directly with `FileHelper` rather than storing values in `store.json` and passing them as environment variables. A direct FileModel provides:

- **Two-way binding**: Actions can read and write the upstream config file directly.
- **Simpler main.ts**: Mount the config file from the volume into the subcontainer. No need to read and regenerate it.
- **Easy user configuration**: Exposing config options via Actions is as simple as `configToml.merge(effects, { key: newValue })`.

Use `store.json` only for internal package state that has no upstream config file equivalent (e.g., a generated PostgreSQL password that the upstream service doesn't read from its own config file).

```typescript
// GOOD: Model the upstream config directly
export const configToml = FileHelper.toml({ base: sdk.volumes['my-data'], subpath: 'config.toml' }, shape)

// In main.ts, mount the volume so the config file is accessible in the subcontainer.
const appSub = sdk.SubContainer.of(
  effects,
  { imageId: 'my-app' },
  sdk.Mounts.of().mountVolume({
    volumeId: 'my-data',
    subpath: 'config.toml',
    mountpoint: '/etc/my-app/config.toml',
    readonly: false,
    type: 'file',
  }),
  'my-app-sub',
)

// Reactive read triggers daemon restart when config changes (e.g. via actions)
await configToml.read(c => c.some_mutable_setting).const(effects)

// In an action, toggle a setting directly
await configToml.merge(effects, { allow_registration: !current })
```

> [!WARNING]
> Do NOT read a FileModel in main.ts and then write it back to the subcontainer rootfs. The file already lives on the volume — just mount it.
