# Back Up and Restore Data

Every StartOS package must define a backup strategy. The SDK provides builders for common patterns: simple volume snapshots, PostgreSQL dumps, MySQL dumps, and incremental rsync for large datasets. StartOS always runs the backup with the service stopped — when a backup begins it first stops the service if it was running, performs the backup, then restarts it afterward, but only if it had been running.

## Solution

Use `sdk.setupBackups()` with the appropriate builder. `sdk.Backups.ofVolumes('main')` for simple volume snapshots. `sdk.Backups.withPgDump()` for PostgreSQL (handles dump and restore). `sdk.Backups.withMysqlDump()` for MySQL/MariaDB. Chain `.addVolume('name')` for additional volumes. Use `.addSync({ dataPath, backupPath })` instead of `.addVolume()` for large, mostly-unchanged datasets (user uploads, media) — rsync is incremental and much faster than full volume copies.

> [!NOTE]
> Because the service is stopped for the duration of the backup, your backup logic runs against a quiescent volume — nothing is writing to the data while it is copied or dumped. StartOS restarts the service automatically once the backup finishes, but only if it was running when the backup began; a service that was already stopped stays stopped.

> [!NOTE]
> Progress is reported per phase, weighted so the rsync copy — the slow part — dominates the bar (`DEFAULT_SYNC_WEIGHT` = 80 per sync, `DEFAULT_HOOK_WEIGHT` = 10 per pre/post hook). A pre/post phase appears only when you set that hook. Override any phase's weight with the optional `weight` argument to `setPreBackup` / `setPostBackup` / `setPreRestore` / `setPostRestore`, or the `weight` field on a sync (`addSync` / `addVolume`).

**Reference:** [Main](main.md) · [File Models](file-models.md)

## Examples

See `startos/backups.ts` in: [hello-world](https://github.com/Start9Labs/hello-world-startos) (simple volume), [spliit](https://github.com/Start9Labs/spliit-startos) (pg_dump), [ghost](https://github.com/Start9Labs/ghost-startos) (mysqldump), [nextcloud](https://github.com/Start9Labs/nextcloud-startos) (pg_dump + rsync), [immich](https://github.com/Start9Labs/immich-startos) (pg_dump + rsync)
