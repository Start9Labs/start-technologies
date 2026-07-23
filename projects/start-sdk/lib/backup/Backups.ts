import * as T from '@start9labs/start-core/types'
import * as child_process from 'child_process'
import * as fs from 'fs/promises'
import { Affine, asError } from '../util'
import { InitKind, InitScript } from '@start9labs/start-core/inits'
import { SubContainer, execFile } from '../util/SubContainer'
import { Mounts } from '../mainFn/Mounts'
import {
  FullProgressTracker,
  PhaseHandle,
} from '@start9labs/start-core/util/FullProgressTracker'

const BACKUP_HOST_PATH = '/media/startos/backup'
const BACKUP_CONTAINER_MOUNT = '/backup-target'

/** A password value, or a function that returns one. Functions are resolved lazily (only during restore). */
export type LazyPassword = string | (() => string | Promise<string>) | null

async function resolvePassword(pw: LazyPassword): Promise<string | null> {
  if (pw === null) return null
  return typeof pw === 'function' ? pw() : pw
}

/** Configuration for PostgreSQL dump-based backup */
export type PgDumpConfig<M extends T.SDKManifest> = {
  /** Image ID of the PostgreSQL container (e.g. 'postgres') */
  imageId: keyof M['images'] & T.ImageId
  /** Volume ID containing the PostgreSQL data directory */
  dbVolume: M['volumes'][number]
  /** Volume mountpoint (e.g. '/var/lib/postgresql') */
  mountpoint: string
  /** Subpath from mountpoint to PGDATA (e.g. '/data', '/18/docker') */
  pgdataPath: string
  /** PostgreSQL database name to dump */
  database: string
  /** PostgreSQL user */
  user: string
  /** PostgreSQL password (for restore). Can be a string, a function that returns one (resolved lazily after volumes are restored), or null for trust auth. */
  password: LazyPassword
  /** Additional initdb arguments (e.g. ['--data-checksums']) */
  initdbArgs?: string[]
  /** Additional options passed to `pg_ctl start -o` (e.g. '-c shared_preload_libraries=vectorchord'). Appended after `-c listen_addresses=`. */
  pgOptions?: string
  /** Milliseconds to wait for PostgreSQL to accept connections before failing (default 60000). Raise for large clusters that need longer to start or run crash recovery. */
  readyTimeout?: number
}

/** Configuration for MySQL/MariaDB dump-based backup */
export type MysqlDumpConfig<M extends T.SDKManifest> = {
  /** Image ID of the MySQL/MariaDB container (e.g. 'mysql', 'mariadb') */
  imageId: keyof M['images'] & T.ImageId
  /** Volume ID containing the MySQL data directory */
  dbVolume: M['volumes'][number]
  /** Path to MySQL data directory within the container (typically '/var/lib/mysql') */
  datadir: string
  /** MySQL database name to dump */
  database: string
  /** MySQL user for dump operations */
  user: string
  /** MySQL password. Can be a string or a function that returns one — functions are resolved lazily after volumes are restored. */
  password: LazyPassword
  /** Database engine: 'mysql' uses --initialize-insecure, 'mariadb' uses mysql_install_db */
  engine: 'mysql' | 'mariadb'
  /** Custom readiness check command (default: ['mysqladmin', 'ping', ...]) */
  readyCommand?: string[]
  /** Additional options passed to `mysqld` on startup (e.g. '--innodb-buffer-pool-size=256M'). Appended after `--bind-address=127.0.0.1`. */
  mysqldOptions?: string[]
  /** Milliseconds to wait for MySQL/MariaDB to become ready before failing (default 30000). Raise for large data directories that need longer to initialize or run crash recovery. */
  readyTimeout?: number
}

/**
 * Bind-mount the host backup directory (`/media/startos/backup`) into a
 * SubContainer's rootfs.
 *
 * Native 0.4 packages mount at the SDK's `/backup-target`; legacy
 * SystemForEmbassy packages pass the manifest-declared mountpoint so the
 * docker backup/restore procedure finds its `backup`-type volume where it
 * expects. Both paths share this single rbind so legacy backups go through
 * the same mechanism as native ones.
 */
export async function mountBackupTarget(
  rootfs: string,
  containerPath: string = BACKUP_CONTAINER_MOUNT,
) {
  const normalized = containerPath.startsWith('/')
    ? containerPath
    : `/${containerPath}`
  const target = `${rootfs}${normalized}`
  await fs.mkdir(target, { recursive: true })
  await execFile('mount', ['--rbind', BACKUP_HOST_PATH, target])
}

/** Default rsync options used for backup and restore operations */
export const DEFAULT_OPTIONS: T.SyncOptions = {
  delete: true,
  exclude: [],
}
/**
 * Default progress weight of each rsync sync phase. rsync dominates a backup's
 * runtime, so it dominates the progress bar relative to the pre/post hooks.
 */
export const DEFAULT_SYNC_WEIGHT = 80
/** Default progress weight of a pre/post backup or restore hook phase. */
export const DEFAULT_HOOK_WEIGHT = 10
/** A single source-to-destination sync pair for backup and restore */
export type BackupSync<Volumes extends string> = {
  dataPath: `/media/startos/volumes/${Volumes}/${string}`
  backupPath: `/media/startos/backup/${string}`
  options?: Partial<T.SyncOptions>
  backupOptions?: Partial<T.SyncOptions>
  restoreOptions?: Partial<T.SyncOptions>
  /** Progress weight of this sync's phase (default {@link DEFAULT_SYNC_WEIGHT}). */
  weight?: number
}

/** Effects type narrowed for backup/restore contexts, preventing reuse outside that scope */
export type BackupEffects = T.Effects & Affine<'Backups'>

/** A pre/post backup or restore hook, given a handle to its own progress phase. */
export type BackupHook = (
  effects: BackupEffects,
  progress: PhaseHandle,
) => Promise<void>

/**
 * Configures backup and restore operations using rsync.
 *
 * Supports syncing entire volumes or custom path pairs, with optional pre/post hooks
 * for both backup and restore phases. Implements {@link InitScript} so it can be used
 * as a restore-init step in `setupInit`.
 *
 * @typeParam M - The service manifest type
 */
export class Backups<M extends T.SDKManifest> implements InitScript {
  private constructor(
    private options = DEFAULT_OPTIONS,
    private restoreOptions: Partial<T.SyncOptions> = {},
    private backupOptions: Partial<T.SyncOptions> = {},
    private backupSet = [] as BackupSync<M['volumes'][number]>[],
  ) {}

  private preBackup?: BackupHook
  private postBackup?: BackupHook
  private preRestore?: BackupHook
  private postRestore?: BackupHook
  private preBackupWeight = DEFAULT_HOOK_WEIGHT
  private postBackupWeight = DEFAULT_HOOK_WEIGHT
  private preRestoreWeight = DEFAULT_HOOK_WEIGHT
  private postRestoreWeight = DEFAULT_HOOK_WEIGHT

  /**
   * Create a Backups configuration that backs up entire volumes by name.
   * Each volume is synced to a corresponding directory under `/media/startos/backup/volumes/`.
   * @param volumeNames - One or more volume IDs from the manifest
   */
  static ofVolumes<M extends T.SDKManifest = never>(
    ...volumeNames: Array<M['volumes'][number]>
  ): Backups<M> {
    return Backups.ofSyncs(
      ...volumeNames.map(srcVolume => ({
        dataPath: `/media/startos/volumes/${srcVolume}/` as const,
        backupPath: `/media/startos/backup/volumes/${srcVolume}/` as const,
      })),
    )
  }

  /**
   * Create a Backups configuration from explicit source/destination sync pairs.
   * @param syncs - Array of `{ dataPath, backupPath }` objects with optional per-sync options
   */
  static ofSyncs<M extends T.SDKManifest = never>(
    ...syncs: BackupSync<M['volumes'][number]>[]
  ) {
    return syncs.reduce((acc, x) => acc.addSync(x), new Backups<M>())
  }

  /**
   * Create an empty Backups configuration with custom default rsync options.
   * Chain `.addVolume()` or `.addSync()` to add sync targets.
   * @param options - Partial rsync options to override defaults (e.g. `{ exclude: ['cache'] }`)
   */
  static withOptions<M extends T.SDKManifest = never>(
    options?: Partial<T.SyncOptions>,
  ) {
    return new Backups<M>({ ...DEFAULT_OPTIONS, ...options })
  }

  /**
   * Configure PostgreSQL dump-based backup for a volume.
   *
   * Instead of rsyncing the raw PostgreSQL data directory (which is slow and error-prone),
   * this uses `pg_dump` to create a logical dump before backup and `pg_restore` to rebuild
   * the database after restore.
   *
   * The dump file is written directly to the backup target — no data duplication on disk.
   *
   * @returns A configured Backups instance with pre/post hooks. Chain `.addVolume()` or
   * `.addSync()` to include additional volumes/paths in the backup.
   */
  static withPgDump<M extends T.SDKManifest = never>(
    config: PgDumpConfig<M>,
  ): Backups<M> {
    const {
      imageId,
      dbVolume,
      mountpoint,
      pgdataPath,
      database,
      user,
      password,
      initdbArgs = [],
      pgOptions,
      readyTimeout = 60_000,
    } = config
    const pgdata = `${mountpoint}${pgdataPath}`
    const dumpFile = `${BACKUP_CONTAINER_MOUNT}/${database}-db.dump`
    // pg_dump's writes are silently dropped on the backup-fs FUSE mount —
    // pg_dump exits 0 but the file stays 0 bytes. `cp` writes through the
    // FUSE correctly, so we dump to a non-FUSE path inside the subcontainer
    // rootfs and `cp` the result through. Likewise on restore, `cp` the dump
    // off the FUSE before pg_restore reads it.
    const tmpDumpFile = `/tmp/${database}-db.dump`

    function dbMounts() {
      return Mounts.of<M>().mountVolume({
        volumeId: dbVolume,
        mountpoint: mountpoint,
        readonly: false,
        subpath: null,
      })
    }

    async function startPg(
      sub: {
        exec(cmd: string[], opts?: any): Promise<{ exitCode: number | null }>
        execFail(
          cmd: string[],
          opts?: any,
          timeout?: number | null,
        ): Promise<any>
      },
      label: string,
    ) {
      await sub.exec(['rm', '-f', `${pgdata}/postmaster.pid`], {
        user: 'postgres',
      })
      await sub.exec(['mkdir', '-p', '/var/run/postgresql'], {
        user: 'root',
      })
      await sub.exec(['chown', 'postgres:postgres', '/var/run/postgresql'], {
        user: 'root',
      })
      console.log(`[${label}] starting postgres`)
      const pgStartOpts = pgOptions
        ? `-c listen_addresses= ${pgOptions}`
        : '-c listen_addresses='
      await sub.execFail(['pg_ctl', 'start', '-D', pgdata, '-o', pgStartOpts], {
        user: 'postgres',
      })
      for (let elapsed = 0; elapsed < readyTimeout; elapsed += 1000) {
        const { exitCode } = await sub.exec(['pg_isready', '-U', user], {
          user: 'postgres',
        })
        if (exitCode === 0) {
          console.log(`[${label}] postgres is ready`)
          return
        }
        await new Promise(r => setTimeout(r, 1000))
      }
      throw new Error(
        `PostgreSQL failed to become ready within ${readyTimeout / 1000} seconds`,
      )
    }

    return new Backups<M>()
      .setPreBackup(async effects => {
        await SubContainer.withTemp<M, void, BackupEffects>(
          effects,
          { imageId },
          dbMounts() as any,
          'pg-dump',
          async sub => {
            console.log('[pg-dump] mounting backup target')
            await mountBackupTarget(sub.rootfs)
            await sub.execFail(['touch', tmpDumpFile], { user: 'root' })
            await sub.execFail(['chown', 'postgres:postgres', tmpDumpFile], {
              user: 'root',
            })
            await startPg(sub, 'pg-dump')
            console.log('[pg-dump] dumping database')
            await sub.execFail(
              ['pg_dump', '-U', user, '-Fc', '-f', tmpDumpFile, database],
              { user: 'postgres' },
              null,
            )
            console.log('[pg-dump] copying dump to backup target')
            await sub.execFail(['cp', tmpDumpFile, dumpFile], { user: 'root' })
            console.log('[pg-dump] stopping postgres')
            await sub.execFail(['pg_ctl', 'stop', '-D', pgdata, '-w'], {
              user: 'postgres',
            })
            console.log('[pg-dump] complete')
          },
        )
      })
      .setPostRestore(async effects => {
        const resolvedPassword = await resolvePassword(password)
        await SubContainer.withTemp<M, void, BackupEffects>(
          effects,
          { imageId },
          dbMounts() as any,
          'pg-restore',
          async sub => {
            await mountBackupTarget(sub.rootfs)
            // Stage the dump off the FUSE before pg_restore reads it — see
            // the comment on `tmpDumpFile` above.
            await sub.execFail(['cp', dumpFile, tmpDumpFile], { user: 'root' })
            await sub.execFail(['chown', 'postgres:postgres', tmpDumpFile], {
              user: 'root',
            })
            await sub.execFail(
              ['chown', '-R', 'postgres:postgres', mountpoint],
              { user: 'root' },
            )
            await sub.execFail(
              ['initdb', '-D', pgdata, '-U', user, ...initdbArgs],
              { user: 'postgres' },
            )
            await startPg(sub, 'pg-restore')
            await sub.execFail(['createdb', '-U', user, database], {
              user: 'postgres',
            })
            await sub.execFail(
              [
                'pg_restore',
                '-U',
                user,
                '-d',
                database,
                '--no-owner',
                '--no-privileges',
                tmpDumpFile,
              ],
              { user: 'postgres' },
              null,
            )
            if (resolvedPassword !== null) {
              await sub.execFail(
                [
                  'psql',
                  '-U',
                  user,
                  '-d',
                  database,
                  '-c',
                  `ALTER USER ${user} WITH PASSWORD '${resolvedPassword}'`,
                ],
                { user: 'postgres' },
              )
            }
            await sub.execFail(['pg_ctl', 'stop', '-D', pgdata, '-w'], {
              user: 'postgres',
            })
          },
        )
      })
  }

  /**
   * Configure MySQL/MariaDB dump-based backup for a volume.
   *
   * Instead of rsyncing the raw MySQL data directory (which is slow and error-prone),
   * this uses `mysqldump` to create a logical dump before backup and `mysql` to restore
   * the database after restore.
   *
   * The dump file is stored temporarily in `dumpVolume` during backup and cleaned up afterward.
   *
   * @returns A configured Backups instance with pre/post hooks. Chain `.addVolume()` or
   * `.addSync()` to include additional volumes/paths in the backup.
   */
  static withMysqlDump<M extends T.SDKManifest = never>(
    config: MysqlDumpConfig<M>,
  ): Backups<M> {
    const {
      imageId,
      dbVolume,
      datadir,
      database,
      user,
      password,
      engine,
      readyCommand,
      mysqldOptions = [],
      readyTimeout = 30_000,
    } = config
    const dumpFile = `${BACKUP_CONTAINER_MOUNT}/${database}-db.dump`
    // See the comment on `tmpDumpFile` in withPgDump — backup-fs FUSE
    // silently drops mysqldump's writes, so stage the dump in the
    // subcontainer rootfs and `cp` through.
    const tmpDumpFile = `/tmp/${database}-db.dump`

    function dbMounts() {
      return Mounts.of<M>().mountVolume({
        volumeId: dbVolume,
        mountpoint: datadir,
        readonly: false,
        subpath: null,
      })
    }

    async function waitForMysql(
      sub: { exec(cmd: string[]): Promise<{ exitCode: number | null }> },
      cmd: string[],
    ) {
      for (let elapsed = 0; elapsed < readyTimeout; elapsed += 1000) {
        const { exitCode } = await sub.exec(cmd)
        if (exitCode === 0) return
        await new Promise(r => setTimeout(r, 1000))
      }
      throw new Error(
        `MySQL/MariaDB failed to become ready within ${readyTimeout / 1000} seconds`,
      )
    }

    async function startMysql(sub: {
      exec(cmd: string[], opts?: any): Promise<{ exitCode: number | null }>
      execFail(cmd: string[], opts?: any, timeout?: number | null): Promise<any>
    }) {
      if (engine === 'mariadb') {
        // MariaDB doesn't support --daemonize; fire-and-forget the exec
        sub
          .exec(
            [
              'mysqld',
              '--user=mysql',
              `--datadir=${datadir}`,
              '--bind-address=127.0.0.1',
              ...mysqldOptions,
            ],
            { user: 'root' },
          )
          .catch(e =>
            console.error('[mysql-backup] mysqld exited unexpectedly:', e),
          )
      } else {
        await sub.execFail(
          [
            'mysqld',
            '--user=mysql',
            `--datadir=${datadir}`,
            '--bind-address=127.0.0.1',
            '--daemonize',
            ...mysqldOptions,
          ],
          { user: 'root' },
          null,
        )
      }
    }

    async function stopMysql(sub: {
      exec(cmd: string[], opts?: any): Promise<{ exitCode: number | null }>
      execFail(cmd: string[], opts?: any, timeout?: number | null): Promise<any>
    }) {
      // SIGTERM mysqld and wait for it to finish flushing before teardown.
      // A killed-but-unreaped mysqld lingers as a zombie that keeps its PID,
      // so `tail --pid`/`kill -0` would block here forever — treat the zombie
      // ('Z') state, or a vanished PID, as "fully exited". Bounded SIGKILL
      // fallback guarantees we can never deadlock the backup.
      await sub.execFail(
        [
          'sh',
          '-c',
          [
            'PID="$(cat /var/run/mysqld/mysqld.pid)"',
            'kill "$PID" 2>/dev/null || exit 0',
            'i=0',
            'while [ -e "/proc/$PID" ]; do',
            '  case "$(cut -d" " -f3 "/proc/$PID/stat" 2>/dev/null)" in Z|"") break ;; esac',
            '  i=$((i + 1))',
            '  [ "$i" -ge 600 ] && { kill -9 "$PID" 2>/dev/null; break; }',
            '  sleep 0.2',
            'done',
          ].join('\n'),
        ],
        { user: 'root' },
        null,
      )
    }

    return new Backups<M>()
      .setPreBackup(async effects => {
        const pw = await resolvePassword(password)
        const readyCmd = readyCommand || [
          'mysqladmin',
          'ping',
          '-u',
          user,
          ...(pw !== null ? [`-p${pw}`] : []),
          '--silent',
        ]
        await SubContainer.withTemp<M, void, BackupEffects>(
          effects,
          { imageId },
          dbMounts() as any,
          'mysql-dump',
          async sub => {
            await mountBackupTarget(sub.rootfs)
            await sub.exec(['mkdir', '-p', '/var/run/mysqld'], {
              user: 'root',
            })
            await sub.exec(['chown', 'mysql:mysql', '/var/run/mysqld'], {
              user: 'root',
            })
            if (engine === 'mysql') {
              await sub.execFail(['chown', '-R', 'mysql:mysql', datadir], {
                user: 'root',
              })
            }
            await startMysql(sub)
            await waitForMysql(sub, readyCmd)
            await sub.execFail(
              [
                'mysqldump',
                '-u',
                user,
                ...(pw !== null ? [`-p${pw}`] : []),
                '--single-transaction',
                `--result-file=${tmpDumpFile}`,
                database,
              ],
              { user: 'root' },
              null,
            )
            await sub.execFail(['cp', tmpDumpFile, dumpFile], { user: 'root' })
            await stopMysql(sub)
          },
        )
      })
      .setPostRestore(async effects => {
        const pw = await resolvePassword(password)
        await SubContainer.withTemp<M, void, BackupEffects>(
          effects,
          { imageId },
          dbMounts() as any,
          'mysql-restore',
          async sub => {
            await mountBackupTarget(sub.rootfs)
            await sub.exec(['mkdir', '-p', '/var/run/mysqld'], {
              user: 'root',
            })
            await sub.exec(['chown', 'mysql:mysql', '/var/run/mysqld'], {
              user: 'root',
            })
            // Initialize fresh data directory
            if (engine === 'mariadb') {
              await sub.execFail(
                ['mysql_install_db', '--user=mysql', `--datadir=${datadir}`],
                { user: 'root' },
              )
            } else {
              await sub.execFail(
                [
                  'mysqld',
                  '--initialize-insecure',
                  '--user=mysql',
                  `--datadir=${datadir}`,
                ],
                { user: 'root' },
              )
            }
            await startMysql(sub)
            // After fresh init, root has no password
            await waitForMysql(sub, [
              'mysqladmin',
              'ping',
              '-u',
              'root',
              '--silent',
            ])
            // Create database, user, and set password
            const grantSql =
              pw !== null
                ? `CREATE DATABASE IF NOT EXISTS \`${database}\`; CREATE USER IF NOT EXISTS '${user}'@'localhost' IDENTIFIED BY '${pw}'; GRANT ALL ON \`${database}\`.* TO '${user}'@'localhost'; ALTER USER 'root'@'localhost' IDENTIFIED BY '${pw}'; FLUSH PRIVILEGES;`
                : `CREATE DATABASE IF NOT EXISTS \`${database}\`; CREATE USER IF NOT EXISTS '${user}'@'localhost'; GRANT ALL ON \`${database}\`.* TO '${user}'@'localhost'; FLUSH PRIVILEGES;`
            await sub.execFail(['mysql', '-u', 'root', '-e', grantSql], {
              user: 'root',
            })
            // Stage the dump off the FUSE before mysql reads it — see
            // the comment on `tmpDumpFile` above.
            await sub.execFail(['cp', dumpFile, tmpDumpFile], { user: 'root' })
            // Restore from dump
            await sub.execFail(
              [
                'sh',
                '-c',
                `mysql -u root ${pw !== null ? `-p'${pw}'` : ''} ${database} < ${tmpDumpFile}`,
              ],
              { user: 'root' },
              null,
            )
            await stopMysql(sub)
          },
        )
      })
  }

  /**
   * Override the default rsync options for both backup and restore.
   * @param options - Partial rsync options to merge with current defaults
   */
  setOptions(options?: Partial<T.SyncOptions>) {
    this.options = {
      ...this.options,
      ...options,
    }
    return this
  }

  /**
   * Override rsync options used only during backup (not restore).
   * @param options - Partial rsync options for the backup phase
   */
  setBackupOptions(options?: Partial<T.SyncOptions>) {
    this.backupOptions = {
      ...this.backupOptions,
      ...options,
    }
    return this
  }

  /**
   * Override rsync options used only during restore (not backup).
   * @param options - Partial rsync options for the restore phase
   */
  setRestoreOptions(options?: Partial<T.SyncOptions>) {
    this.restoreOptions = {
      ...this.restoreOptions,
      ...options,
    }
    return this
  }

  /**
   * Register a hook to run before backup rsync begins (e.g. dump a database).
   * @param fn - Async function receiving backup-scoped effects and a progress tracker for this hook
   */
  setPreBackup(fn: BackupHook, weight: number = DEFAULT_HOOK_WEIGHT) {
    this.preBackup = fn
    this.preBackupWeight = weight
    return this
  }

  /**
   * Register a hook to run after backup rsync completes.
   * @param fn - Async function receiving backup-scoped effects and a progress tracker for this hook
   */
  setPostBackup(fn: BackupHook, weight: number = DEFAULT_HOOK_WEIGHT) {
    this.postBackup = fn
    this.postBackupWeight = weight
    return this
  }

  /**
   * Register a hook to run before restore rsync begins.
   * @param fn - Async function receiving backup-scoped effects and a progress tracker for this hook
   */
  setPreRestore(fn: BackupHook, weight: number = DEFAULT_HOOK_WEIGHT) {
    this.preRestore = fn
    this.preRestoreWeight = weight
    return this
  }

  /**
   * Register a hook to run after restore rsync completes.
   * @param fn - Async function receiving backup-scoped effects and a progress tracker for this hook
   */
  setPostRestore(fn: BackupHook, weight: number = DEFAULT_HOOK_WEIGHT) {
    this.postRestore = fn
    this.postRestoreWeight = weight
    return this
  }

  /**
   * Add a volume to the backup set by its ID.
   * @param volume - The volume ID from the manifest
   * @param options - Optional per-volume rsync overrides
   */
  addVolume(
    volume: M['volumes'][number],
    options?: Partial<{
      options: T.SyncOptions
      backupOptions: T.SyncOptions
      restoreOptions: T.SyncOptions
      weight: number
    }>,
  ) {
    return this.addSync({
      dataPath: `/media/startos/volumes/${volume}/` as const,
      backupPath: `/media/startos/backup/volumes/${volume}/` as const,
      ...options,
    })
  }

  /**
   * Add a custom sync pair to the backup set.
   * @param sync - A `{ dataPath, backupPath }` object with optional per-sync rsync options
   */
  addSync(sync: BackupSync<M['volumes'][0]>) {
    this.backupSet.push(sync)
    return this
  }

  /**
   * Execute the backup: runs pre-hook, rsyncs all configured paths, saves the data version, then runs post-hook.
   * @param effects - The effects context
   */
  async createBackup(effects: T.Effects) {
    // Root tracker reports to the backup progress UI via setBackupProgress,
    // with the effects context baked into the sink. Phase updates auto-sync in
    // the background; we only flush at the end.
    const tracker = new FullProgressTracker(progress =>
      effects.setBackupProgress({ progress }),
    )
    const preHook = this.preBackup
      ? tracker.addPhase('pre-backup', this.preBackupWeight)
      : undefined
    const syncs = this.backupSet.map(s =>
      tracker.addPhase(
        `rsync:${s.backupPath}`,
        s.weight ?? DEFAULT_SYNC_WEIGHT,
      ),
    )
    const postHook = this.postBackup
      ? tracker.addPhase('post-backup', this.postBackupWeight)
      : undefined

    if (preHook) {
      await this.preBackup!(effects as BackupEffects, preHook)
      preHook.complete()
    }

    for (let i = 0; i < this.backupSet.length; i++) {
      const item = this.backupSet[i]!
      const phase = syncs[i]!
      phase.start()
      phase.setTotal(100)
      phase.setDone(0)

      const rsyncResults = await runRsync({
        srcPath: item.dataPath,
        dstPath: item.backupPath,
        options: {
          ...this.options,
          ...this.backupOptions,
          ...item.options,
          ...item.backupOptions,
        },
      })
      // Poll rsync's parsed percentage; setDone auto-syncs to the host. Cap at
      // 99 until wait() resolves so the bar never claims "done" before exit.
      const interval = setInterval(async () => {
        const pct = await rsyncResults.progress()
        phase.setDone(Math.min(99, Math.floor(pct)))
      }, 500)
      try {
        await rsyncResults.wait()
      } finally {
        clearInterval(interval)
      }
      phase.complete()
    }

    const dataVersion = await effects.getDataVersion()
    if (dataVersion)
      await fs.writeFile('/media/startos/backup/dataVersion.txt', dataVersion, {
        encoding: 'utf-8',
      })
    if (postHook) {
      await this.postBackup!(effects as BackupEffects, postHook)
      postHook.complete()
    }
    // Don't mark the tracker complete: the OS backup harness holds this
    // package's phase open until the s9pk image finishes writing, so it reports
    // 100%, not "done".
    await tracker.sync()
    return
  }

  async init(
    effects: T.Effects,
    kind: InitKind,
    progress?: FullProgressTracker,
  ): Promise<void> {
    if (kind === 'restore') {
      await this.restoreBackup(effects, progress)
    }
  }

  /**
   * Execute the restore: runs pre-hook, rsyncs all configured paths from backup to data, restores the data version, then runs post-hook.
   * Restore runs as part of init, so progress reports through the init tracker.
   * @param effects - The effects context
   * @param progress - Tracker from the init harness (falls back to a no-op tracker)
   */
  async restoreBackup(effects: T.Effects, progress?: FullProgressTracker) {
    // Restore runs as part of init, so progress reports through the init
    // tracker passed in. When called directly, fall back to a tracker wired to
    // setInitProgress. Phase updates auto-sync; we flush at the end.
    const tracker =
      progress ??
      new FullProgressTracker(p => effects.setInitProgress({ progress: p }))
    const preHook = this.preRestore
      ? tracker.addPhase('pre-restore', this.preRestoreWeight)
      : undefined
    const syncs = this.backupSet.map(s =>
      tracker.addPhase(
        `restore:${s.dataPath}`,
        s.weight ?? DEFAULT_SYNC_WEIGHT,
      ),
    )
    const postHook = this.postRestore
      ? tracker.addPhase('post-restore', this.postRestoreWeight)
      : undefined

    if (preHook) {
      await this.preRestore!(effects as BackupEffects, preHook)
      preHook.complete()
    }

    for (let i = 0; i < this.backupSet.length; i++) {
      const item = this.backupSet[i]!
      const phase = syncs[i]!
      phase.start()
      phase.setTotal(100)
      phase.setDone(0)

      const rsyncResults = await runRsync({
        srcPath: item.backupPath,
        dstPath: item.dataPath,
        options: {
          ...this.options,
          ...this.restoreOptions,
          ...item.options,
          ...item.restoreOptions,
        },
      })
      const interval = setInterval(async () => {
        const pct = await rsyncResults.progress()
        phase.setDone(Math.min(99, Math.floor(pct)))
      }, 500)
      try {
        await rsyncResults.wait()
      } finally {
        clearInterval(interval)
      }
      phase.complete()
    }
    const dataVersion = await fs
      .readFile('/media/startos/backup/dataVersion.txt', {
        encoding: 'utf-8',
      })
      .catch(_ => null)
    if (dataVersion) await effects.setDataVersion({ version: dataVersion })
    if (postHook) {
      await this.postRestore!(effects as BackupEffects, postHook)
      postHook.complete()
    }
    await tracker.sync()
    return
  }
}

async function runRsync(rsyncOptions: {
  srcPath: string
  dstPath: string
  options: T.SyncOptions
}): Promise<{
  id: () => Promise<string>
  wait: () => Promise<null>
  progress: () => Promise<number>
}> {
  const { srcPath, dstPath, options } = rsyncOptions

  await fs.mkdir(dstPath, { recursive: true })

  const command = 'rsync'
  const args: string[] = []
  if (options.delete) {
    args.push('--delete')
  }
  for (const exclude of options.exclude) {
    args.push(`--exclude=${exclude}`)
  }
  args.push('-rlptgoAXH')
  args.push('--partial')
  args.push('--inplace')
  args.push('--timeout=300')
  args.push('--info=progress2')
  // --no-inc-recursive would give accurate progress percentages (since rsync
  // knows the full file list up front), but it forces a full pre-scan that
  // causes timeouts on large backups. If we start surfacing progress to users,
  // do a raw file count up front and compute percentage from bytes/files seen
  // instead of relying on rsync's own percentage.
  args.push(srcPath)
  args.push(dstPath)
  const spawned = child_process.spawn(command, args, { detached: true })
  let percentage = 0.0
  spawned.stdout.on('data', (data: unknown) => {
    const lines = String(data).replace(/\r/g, '\n').split('\n')
    for (const line of lines) {
      const parsed = /([0-9.]+)%/.exec(line)?.[1]
      if (!parsed) {
        if (line) console.log(line)
        continue
      }
      percentage = Number.parseFloat(parsed)
    }
  })

  let stderr = ''

  spawned.stderr.on('data', (data: string | Buffer) => {
    const errString = data.toString('utf-8')
    stderr += errString
    console.error(`Backups.runAsync`, asError(errString))
  })

  const id = async () => {
    const pid = spawned.pid
    if (pid === undefined) {
      throw new Error('rsync process has no pid')
    }
    return String(pid)
  }
  const waitPromise = new Promise<null>((resolve, reject) => {
    spawned.on('exit', (code: any) => {
      if (code === 0) {
        resolve(null)
      } else {
        reject(new Error(`rsync exited with code ${code}\n${stderr}`))
      }
    })
  })
  const wait = () => waitPromise
  const progress = () => Promise.resolve(percentage)
  return { id, wait, progress }
}
