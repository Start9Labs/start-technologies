# Container RPC Server Specification

The container runtime exposes a JSON-RPC server over a Unix socket at `/media/startos/rpc/service.sock`.

## Methods

### init

Initialize the runtime and system.

#### params

```ts
{
  id: string,
  kind: "install" | "update" | "restore" | null,
}
```

#### response

`null`

### exit

Shutdown runtime and optionally run exit hooks for a target version.

#### params

```ts
{
  id: string,
  target: string | null,  // ExtendedVersion or VersionRange
}
```

#### response

`null`

### start

Run main method if not already running.

#### params

None

#### response

`null`

### stop

Stop main method by sending SIGTERM to child processes, and SIGKILL after timeout.

#### params

None

#### response

`null`

### execute

Run a specific package procedure.

#### params

```ts
{
  id: string,           // event ID
  procedure: string,    // JSON path (e.g., "/backup/create", "/actions/{name}/run")
  input: any,
  timeout: number | null,
}
```

#### response

`any`

### sandbox

Run a specific package procedure in sandbox mode. Same interface as `execute`.

UNIMPLEMENTED: this feature is planned but does not exist

#### params

```ts
{
  id: string,
  procedure: string,
  input: any,
  timeout: number | null,
}
```

#### response

`any`

### callback

Handle a callback from an effect.

#### params

```ts
{
  id: number,
  args: any[],
}
```

#### response

`null` (no response sent)

### eval

Evaluate a script in the runtime context. Used for debugging.

#### params

```ts
{
  script: string,
}
```

#### response

`any`

## Procedures

The `execute` and `sandbox` methods route to procedures based on the `procedure` path:

| Procedure                  | Description                  |
| -------------------------- | ---------------------------- |
| `/backup/create`           | Create a backup              |
| `/actions/{name}/getInput` | Get input spec for an action |
| `/actions/{name}/run`      | Run an action with input     |

## Errors

A failed call answers with a JSON-RPC error object:

```ts
{
  code: number,
  message: string,   // fixed label for the code
  data: {
    details: string, // what went wrong
    debug?: string,  // stack trace, when there is one
  },
}
```

`code` selects the error; `message` is that code's fixed label and carries no
per-call detail. StartOS reads `code` and renders its own translated label from
it, so anything written into `message` here reaches nobody — the text an
operator reads is `data.details`.

Codes are either a standard JSON-RPC code or a `start-core` `ErrorKind`
discriminant (`shared-libs/crates/start-core/src/error.rs`):

| code     | message                 | raised when                              |
| -------- | ----------------------- | ---------------------------------------- |
| `-32602` | `invalid params`        | the request carries no `method`          |
| `-32601` | `Method not found`      | the `method` is not one of the above     |
| `38`     | `Invalid Request`       | the line is malformed, or dispatch fails |
| `59`     | `Service Runtime Error` | a method or procedure threw              |
