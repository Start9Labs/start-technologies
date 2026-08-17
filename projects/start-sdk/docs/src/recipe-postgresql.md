# Run a PostgreSQL Sidecar

PostgreSQL is the most common database sidecar in StartOS packages. The pattern covers password generation in init, the daemon definition with health check, and backup/restore using the SDK's built-in pg_dump support.

## Solution

Generate a password in `setupOnInit` and store it in a file model. In `setupMain`, create a PostgreSQL subcontainer with `sdk.useEntrypoint(['--listen_addresses=127.0.0.1'])` and pass credentials via env vars. Health-check with `pg_isready`. The app daemon connects via `localhost:5432` and declares `requires: ['postgres']`. For backups, use `sdk.Backups.withPgDump()` which handles dump and restore automatically.

**Reference:** [Main](main.md) · [Initialization](init.md) · [File Models](file-models.md)

## Examples

See `startos/main.ts` and `startos/backups.ts` in: [btcpayserver](https://github.com/Start9Labs/btcpayserver-startos), [immich](https://github.com/Start9Labs/immich-startos), [nextcloud](https://github.com/Start9Labs/nextcloud-startos), [spliit](https://github.com/Start9Labs/spliit-startos)
