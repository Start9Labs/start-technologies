# Back Up and Restore Data

Every StartOS package must define a backup strategy. The SDK provides builders for common patterns: simple volume snapshots, PostgreSQL dumps, MySQL dumps, MariaDB dumps, and incremental rsync for large datasets. StartOS always runs the backup with the service stopped — when a backup begins it first stops the service if it was running, performs the backup, then restarts it afterward, but only if it had been running.

## Solution

Use `sdk.setupBackups()` with the appropriate builder. `sdk.Backups.ofVolumes('main')` for simple volume snapshots. `sdk.Backups.withPgDump()` for PostgreSQL (handles dump and restore). `sdk.Backups.withMysqlDump()` and `sdk.Backups.withMariadbDump()` dump MySQL and MariaDB, though a copied volume is usually the better choice for those (see [Databases](#databases-copy-the-volume-or-dump-it)). Chain `.addVolume('name')` for additional volumes. Use `.addSync({ dataPath, backupPath })` to select paths within a volume rather than copying the whole volume. Both forms use incremental rsync and skip unchanged files.

A restore installs the s9pk stored in the backup, so it runs that version's own backup and restore code; the current version's migrations run on the update after it. Changing the backup set never affects backups taken before the change.

> [!NOTE]
> Because the service is stopped for the duration of the backup, your backup logic runs against a quiescent volume — nothing is writing to the data while it is copied or dumped. StartOS restarts the service automatically once the backup finishes, but only if it was running when the backup began; a service that was already stopped stays stopped.

> [!NOTE]
> Progress is reported per phase, weighted so the rsync copy — the slow part — dominates the bar (`DEFAULT_SYNC_WEIGHT` = 80 per sync, `DEFAULT_HOOK_WEIGHT` = 10 per pre/post hook). A pre/post phase appears only when you set that hook. Override any phase's weight with the optional `weight` argument to `setPreBackup` / `setPostBackup` / `setPreRestore` / `setPostRestore`, or the `weight` field on a sync (`addSync` / `addVolume`).

## Databases: copy the volume or dump it

Copy a database's volume with `ofVolumes`, unless one of the cases below applies or the engine cannot recover its data files after an abrupt shutdown (MyISAM and Aria tables, for example). The backup runs on a stopped service, so nothing writes to the volume during the copy, but the shutdown may not have been clean: stopping escalates to `SIGKILL` after the daemon's timeout, and a crashed service is already stopped. An engine with crash recovery (InnoDB, PostgreSQL) starts from such a copy as it would after a power loss. The restore reinstalls the s9pk that made the backup, so the same engine version reads the files back. A copied volume restores everything the engine keeps there: every database, its accounts and grants, roles, extensions, settings. The restore copies files rather than replaying SQL, though its time still grows with the data size.

A dump captures one database's contents, and the restore rebuilds the engine around it. Whatever lives outside that database, accounts included, comes from the restore code rather than from the backup. Use one when:

- **The data files are tied to the CPU architecture.** A backup can be restored onto a server of another architecture. PostgreSQL does not guarantee that its data files are portable across architectures, so keep `withPgDump` for PostgreSQL. InnoDB data files are documented as cross-platform, so MySQL and MariaDB volumes can be copied.
- **The data directory is much larger than the data in it** (bloat, large indexes) and backup size matters more than restore time.

**Reference:** [Main](main.md) · [File Models](file-models.md)

## Examples

See `startos/backups.ts` in: [hello-world](https://github.com/Start9Labs/hello-world-startos) (simple volume), [spliit](https://github.com/Start9Labs/spliit-startos) (pg_dump), [nextcloud](https://github.com/Start9Labs/nextcloud-startos) (pg_dump + rsync), [immich](https://github.com/Start9Labs/immich-startos) (pg_dump + rsync)
