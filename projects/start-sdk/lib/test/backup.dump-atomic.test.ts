import * as T from '@start9labs/start-core/types'
import { spawn, spawnSync } from 'child_process'
import { createHash } from 'crypto'
import {
  existsSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  readlinkSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'fs'
import * as fs from 'fs/promises'
import { tmpdir } from 'os'
import { join, posix } from 'path'
import { gunzipSync } from 'zlib'
import { Backups } from '../backup/Backups'
import type { BackupHook } from '../backup/Backups'
import { SubContainer } from '../util/SubContainer'

jest.mock('child_process', () => ({
  ...jest.requireActual<typeof import('child_process')>('child_process'),
  execFile: jest.fn(
    (
      _file: string,
      _args: string[],
      callback: (error: null, stdout: string, stderr: string) => void,
    ) => callback(null, '', ''),
  ),
}))

jest.mock('fs/promises', () => {
  const actual = jest.requireActual<typeof import('fs/promises')>('fs/promises')
  return {
    ...actual,
    open: jest.fn((...args: Parameters<typeof actual.open>) =>
      actual.open(...args),
    ),
    rename: jest.fn((...args: Parameters<typeof actual.rename>) =>
      actual.rename(...args),
    ),
    rm: jest.fn((...args: Parameters<typeof actual.rm>) => actual.rm(...args)),
  }
})

const actualFs = jest.requireActual<typeof import('fs/promises')>('fs/promises')
const openMock = fs.open as jest.MockedFunction<typeof fs.open>
const renameMock = fs.rename as jest.MockedFunction<typeof fs.rename>
const rmMock = fs.rm as jest.MockedFunction<typeof fs.rm>

type TestManifest = Omit<T.SDKManifest, 'images' | 'volumes'> & {
  images: Record<'db', T.SDKManifest['images'][string]>
  volumes: ['db']
}

type BuilderCase = {
  name: string
  create: () => Backups<TestManifest>
  dumpFileName: string
  decode?: (buf: Buffer) => Buffer
}

type ExecResult = {
  exitCode: number | null
  exitSignal: NodeJS.Signals | null
  stdout: Buffer
  stderr: Buffer
}

const okResult: ExecResult = {
  exitCode: 0,
  exitSignal: null,
  stdout: Buffer.alloc(0),
  stderr: Buffer.alloc(0),
}

function makeIncompressiblePayload(seed: string, size: number): Buffer {
  const chunks: Buffer[] = []
  for (let i = 0; i * 32 < size; i++) {
    chunks.push(createHash('sha256').update(`${seed}:${i}`).digest())
  }
  return Buffer.concat(chunks, size)
}

const OLD_DUMP = makeIncompressiblePayload('existing-good-dump', 16384)
const NEW_DUMP = makeIncompressiblePayload('replacement-dump', 16384)

const BUILDERS: BuilderCase[] = [
  {
    name: 'withPgDump',
    create: () =>
      Backups.withPgDump<TestManifest>({
        imageId: 'db',
        dbVolume: 'db',
        mountpoint: '/var/lib/postgresql',
        pgdataPath: '/data',
        database: 'app',
        user: 'postgres',
        password: null,
      }),
    dumpFileName: 'app-db.dump',
  },
  {
    name: 'withMysqlDump',
    create: () =>
      Backups.withMysqlDump<TestManifest>({
        imageId: 'db',
        dbVolume: 'db',
        datadir: '/var/lib/mysql',
        database: 'app',
        user: 'app',
        password: 'pw',
      }),
    dumpFileName: 'app-db.dump',
  },
  {
    name: 'withMariadbDump',
    create: () =>
      Backups.withMariadbDump<TestManifest>({
        imageId: 'db',
        dbVolume: 'db',
        datadir: '/var/lib/mysql',
        database: 'app',
        user: 'app',
        password: 'pw',
      }),
    dumpFileName: 'app.sql.gz',
    decode: gunzipSync,
  },
]

function resolveInRootfs(rootfs: string, containerPath: string): string {
  const parts = posix.normalize(containerPath).split('/').filter(Boolean)
  let current = '/'
  for (const part of parts) {
    const next = posix.join(current, part)
    const hostPath = join(rootfs, next)
    let isSymlink = false
    try {
      isSymlink = lstatSync(hostPath).isSymbolicLink()
    } catch {
      isSymlink = false
    }
    if (isSymlink) {
      const target = readlinkSync(hostPath)
      current = target.startsWith('/')
        ? posix.normalize(target)
        : posix.normalize(posix.join(current, target))
    } else {
      current = next
    }
  }
  return join(rootfs, current)
}

function mapContainerPath(arg: string, rootfs: string): string {
  if (arg === '/backup-target' || arg.startsWith('/backup-target/')) {
    return resolveInRootfs(rootfs, arg)
  }
  if (arg === '/tmp' || arg.startsWith('/tmp/')) {
    return resolveInRootfs(rootfs, arg)
  }
  const eq = arg.indexOf('=')
  if (eq !== -1) {
    const val = arg.slice(eq + 1)
    if (val.startsWith('/backup-target/') || val.startsWith('/tmp/')) {
      return arg.slice(0, eq + 1) + resolveInRootfs(rootfs, val)
    }
  }
  return arg
}

function runRealBinary(
  cmd: string,
  args: string[],
  fsizeBlocks?: number,
): ExecResult {
  const res = fsizeBlocks
    ? spawnSync(
        'sh',
        [
          '-c',
          `ulimit -c 0; ulimit -f ${fsizeBlocks}; exec "$@"`,
          '--',
          cmd,
          ...args,
        ],
        { encoding: 'buffer' },
      )
    : spawnSync(cmd, args, { encoding: 'buffer' })
  return {
    exitCode: res.status,
    exitSignal: res.signal,
    stdout: res.stdout ?? Buffer.alloc(0),
    stderr: res.stderr ?? Buffer.alloc(0),
  }
}

type HarnessOptions = {
  failWriteAfterBytes?: number
  failSync?: boolean
  failDirSync?: boolean
  failReaderAfterBytes?: number
  truncateReaderZeroExitAfterBytes?: number
  sizeProbeStdout?: string
  failReaderStderr?: boolean
  failRename?: boolean
  failCleanupRm?: boolean
  seedTarget?: boolean
  beforeRun?: (paths: {
    root: string
    rootfs: string
    backupDir: string
    tmpDir: string
  }) => void
  afterRun?: (paths: {
    root: string
    rootfs: string
    backupDir: string
    tmpDir: string
  }) => void
}

async function runPreBackup(
  testCase: BuilderCase,
  options: HarnessOptions = {},
) {
  const root = mkdtempSync(join(tmpdir(), 'start-sdk-dump-atomic-'))
  try {
    const rootfs = join(root, 'rootfs')
    const backupDir = join(rootfs, 'backup-target')
    const tmpDir = join(rootfs, 'tmp')
    mkdirSync(backupDir, { recursive: true })
    mkdirSync(tmpDir, { recursive: true })

    const targetDump = join(backupDir, testCase.dumpFileName)
    if (options.seedTarget !== false) {
      writeFileSync(targetDump, OLD_DUMP)
    }
    options.beforeRun?.({ root, rootfs, backupDir, tmpDir })

    const events: string[] = []
    let stagedPath: string | null = null
    let renameCalled = false

    openMock.mockImplementation((async (
      filePath: Parameters<typeof fs.open>[0],
      flags?: Parameters<typeof fs.open>[1],
      mode?: Parameters<typeof fs.open>[2],
    ) => {
      const p = String(filePath)
      const handle = await actualFs.open(filePath, flags, mode)
      if (p.startsWith(backupDir) && flags === 'wx') {
        stagedPath = p
        events.push('open-wx')
        if (options.failWriteAfterBytes !== undefined) {
          const limit = options.failWriteAfterBytes
          let writtenTotal = 0
          const realWrite = handle.write.bind(handle)
          jest.spyOn(handle, 'write').mockImplementation((async (
            ...args: unknown[]
          ) => {
            if (writtenTotal >= limit) {
              throw Object.assign(
                new Error('ENOSPC: no space left on device'),
                { code: 'ENOSPC' },
              )
            }
            const buf = args[0] as Buffer
            const offset = (args[1] as number | undefined) ?? 0
            const length =
              (args[2] as number | undefined) ?? buf.length - offset
            const allow = Math.min(length, limit - writtenTotal)
            const res = await realWrite(
              buf,
              offset,
              allow,
              args[3] as number | null,
            )
            writtenTotal += res.bytesWritten
            if (allow < length) {
              throw Object.assign(
                new Error('ENOSPC: no space left on device'),
                { code: 'ENOSPC' },
              )
            }
            return res
          }) as typeof handle.write)
        }
        const realSync = handle.sync.bind(handle)
        jest.spyOn(handle, 'sync').mockImplementation(async () => {
          events.push('file-sync')
          if (options.failSync) {
            throw Object.assign(
              new Error('ENOSPC: backup-fs flush failed on fsync'),
              { code: 'ENOSPC' },
            )
          }
          return realSync()
        })
        const realClose = handle.close.bind(handle)
        jest.spyOn(handle, 'close').mockImplementation(async () => {
          events.push('file-close')
          return realClose()
        })
      } else if (p === backupDir && flags === 'r') {
        const realSync = handle.sync.bind(handle)
        jest.spyOn(handle, 'sync').mockImplementation(async () => {
          events.push('dir-sync')
          if (options.failDirSync) {
            throw Object.assign(new Error('EIO: directory fsync failed'), {
              code: 'EIO',
            })
          }
          return realSync()
        })
      }
      return handle
    }) as typeof fs.open)

    renameMock.mockImplementation(async (oldPath, newPath) => {
      renameCalled = true
      events.push('rename')
      if (options.failRename) {
        throw Object.assign(
          new Error('EIO: simulated pre-replacement rename failure'),
          { code: 'EIO' },
        )
      }
      return actualFs.rename(oldPath, newPath)
    })

    rmMock.mockImplementation(async (target, opts) => {
      if (options.failCleanupRm && String(target).endsWith('.tmp')) {
        throw new Error('simulated cleanup rm failure')
      }
      return actualFs.rm(target, opts)
    })

    let stopMariadb: (() => void) | undefined
    const mariadbServer = new Promise<ExecResult>(resolve => {
      stopMariadb = () => resolve(okResult)
    })

    const runCmd = async (command: string[], failOnError: boolean) => {
      const [bin, ...rawArgs] = command
      const mappedArgs = rawArgs.map(a => mapContainerPath(a, rootfs))
      let res: ExecResult = okResult
      if (
        bin === 'sh' &&
        rawArgs[1]?.includes('stat -Lc %s') &&
        options.sizeProbeStdout !== undefined
      ) {
        res = {
          exitCode: 0,
          exitSignal: null,
          stdout: Buffer.from(options.sizeProbeStdout, 'utf8'),
          stderr: Buffer.alloc(0),
        }
      } else if (
        bin === 'cp' ||
        bin === 'mv' ||
        bin === 'rm' ||
        bin === 'touch' ||
        bin === 'gzip' ||
        (bin === 'sh' && rawArgs[1]?.includes('stat -Lc %s'))
      ) {
        res = runRealBinary(
          bin,
          mappedArgs,
          bin === 'cp' && options.failWriteAfterBytes !== undefined
            ? Math.ceil(options.failWriteAfterBytes / 512)
            : undefined,
        )
      } else if (bin === 'pg_dump') {
        const fIdx = mappedArgs.indexOf('-f')
        writeFileSync(mappedArgs[fIdx + 1]!, NEW_DUMP)
      } else if (bin === 'mysqldump' || bin === 'mariadb-dump') {
        const rf = mappedArgs.find(a => a.startsWith('--result-file='))!
        writeFileSync(rf.slice('--result-file='.length), NEW_DUMP)
      } else if (bin === 'mariadbd') {
        return mariadbServer
      } else if (bin === 'pkill') {
        stopMariadb?.()
      }

      if (failOnError && (res.exitCode !== 0 || res.exitSignal)) {
        throw new Error(
          `${bin} failed (code=${res.exitCode}, signal=${res.exitSignal}): ${res.stderr.toString().trim()}`,
        )
      }
      return res
    }

    const spawnCmd = async (command: string[]) => {
      const [bin, ...rawArgs] = command
      const mappedArgs = rawArgs.map(a => mapContainerPath(a, rootfs))
      if (bin === 'cat' && options.failReaderStderr) {
        const child = spawn(
          'sh',
          [
            '-c',
            'head -c 4096 "$1"; sleep 1',
            '--',
            mappedArgs[mappedArgs.length - 1]!,
          ],
          { stdio: ['ignore', 'pipe', 'pipe'] },
        )
        child.stdout.once('data', () => {
          child.stderr.emit(
            'error',
            Object.assign(new Error('EIO: simulated stderr pipe failure'), {
              code: 'EIO',
            }),
          )
        })
        return child
      }
      if (
        bin === 'cat' &&
        options.truncateReaderZeroExitAfterBytes !== undefined
      ) {
        const srcPath = mappedArgs[mappedArgs.length - 1]!
        return spawn(
          'head',
          ['-c', String(options.truncateReaderZeroExitAfterBytes), srcPath],
          { stdio: ['ignore', 'pipe', 'pipe'] },
        )
      }
      if (bin === 'cat' && options.failReaderAfterBytes !== undefined) {
        const srcPath = mappedArgs[mappedArgs.length - 1]!
        return spawn(
          'sh',
          [
            '-c',
            'head -c "$1" "$2" && echo "simulated read error" >&2 && exit 1',
            '--',
            String(options.failReaderAfterBytes),
            srcPath,
          ],
          { stdio: ['ignore', 'pipe', 'pipe'] },
        )
      }
      return spawn(bin!, mappedArgs, { stdio: ['ignore', 'pipe', 'pipe'] })
    }

    const fakeSub = {
      rootfs,
      exec: jest.fn((cmd: string[]) => runCmd(cmd, false)),
      execFail: jest.fn((cmd: string[]) => runCmd(cmd, true)),
      spawn: jest.fn((cmd: string[]) => spawnCmd(cmd)),
    }

    jest.spyOn(SubContainer, 'withTemp').mockImplementationOnce((async (
      ...args: unknown[]
    ) => {
      const fn = args[4] as (sub: typeof fakeSub) => Promise<void>
      await fn(fakeSub)
    }) as typeof SubContainer.withTemp)

    jest.spyOn(console, 'log').mockImplementation(() => {})
    const backups = testCase.create()
    const preBackup = (backups as unknown as { preBackup: BackupHook })
      .preBackup

    let error: unknown = null
    try {
      await preBackup({} as never, {} as never)
    } catch (e) {
      error = e
    }

    const backupEntries = readdirSync(backupDir).sort()
    const targetStat = existsSync(targetDump) ? lstatSync(targetDump) : null
    const finalTargetBytes =
      targetStat && targetStat.isFile() ? readFileSync(targetDump) : null
    options.afterRun?.({ root, rootfs, backupDir, tmpDir })

    return {
      error,
      events,
      stagedPath,
      renameCalled,
      backupEntries,
      targetStat,
      finalTargetBytes,
    }
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
}

describe('atomic database dump staging', () => {
  beforeEach(() => {
    jest.clearAllMocks()
    jest.useFakeTimers()
  })

  afterEach(() => {
    jest.clearAllTimers()
    jest.useRealTimers()
    jest.restoreAllMocks()
    openMock.mockImplementation((...args) => actualFs.open(...args))
    renameMock.mockImplementation((...args) => actualFs.rename(...args))
    rmMock.mockImplementation((...args) => actualFs.rm(...args))
  })

  test.each(BUILDERS)(
    '$name preserves the previous good dump and cleans up its temp file when streaming writes fail mid-copy',
    async testCase => {
      const outcome = await runPreBackup(testCase, {
        failWriteAfterBytes: 4096,
      })

      expect(outcome.error).toBeInstanceOf(Error)
      expect(outcome.renameCalled).toBe(false)
      expect(outcome.finalTargetBytes?.equals(OLD_DUMP)).toBe(true)
      expect(outcome.backupEntries).toEqual([testCase.dumpFileName])
    },
  )

  test.each(BUILDERS)(
    '$name stages with O_EXCL, fsyncs before close, and atomically replaces the dump on backup success',
    async testCase => {
      const outcome = await runPreBackup(testCase)
      const decode = testCase.decode ?? ((buf: Buffer) => buf)

      expect(outcome.error).toBeNull()
      expect(
        outcome.finalTargetBytes
          ? decode(outcome.finalTargetBytes).equals(NEW_DUMP)
          : false,
      ).toBe(true)
      expect(outcome.backupEntries).toEqual([testCase.dumpFileName])
      expect(outcome.stagedPath).toMatch(
        new RegExp(
          `\\.${testCase.dumpFileName.replace(/\./g, '\\.')}\\.[0-9a-f]{24}\\.tmp$`,
        ),
      )
      expect(outcome.events).toEqual([
        'open-wx',
        'file-sync',
        'file-close',
        'rename',
        'dir-sync',
      ])
    },
  )

  test.each(BUILDERS)(
    '$name preserves the previous good dump and skips rename when fsync fails before close',
    async testCase => {
      const outcome = await runPreBackup(testCase, { failSync: true })

      expect(outcome.error).toBeInstanceOf(Error)
      expect((outcome.error as Error).message).toContain('ENOSPC')
      expect(outcome.renameCalled).toBe(false)
      expect(outcome.finalTargetBytes?.equals(OLD_DUMP)).toBe(true)
      expect(outcome.backupEntries).toEqual([testCase.dumpFileName])
    },
  )

  test('preserves the previous good dump and skips rename when the subcontainer reader exits non-zero', async () => {
    const pgCase = BUILDERS[0]!
    const outcome = await runPreBackup(pgCase, { failReaderAfterBytes: 4096 })

    expect(outcome.error).toBeInstanceOf(Error)
    expect((outcome.error as Error).message).toContain('simulated read error')
    expect(outcome.renameCalled).toBe(false)
    expect(outcome.finalTargetBytes?.equals(OLD_DUMP)).toBe(true)
    expect(outcome.backupEntries).toEqual([pgCase.dumpFileName])
  })

  test.each(BUILDERS)(
    '$name preserves the previous good dump and skips rename when the reader emits a truncated stream and exits 0',
    async testCase => {
      const outcome = await runPreBackup(testCase, {
        truncateReaderZeroExitAfterBytes: 4096,
      })

      expect(outcome.error).toBeInstanceOf(Error)
      expect((outcome.error as Error).message).toContain(
        'Dump stream size mismatch',
      )
      expect(outcome.renameCalled).toBe(false)
      expect(outcome.finalTargetBytes?.equals(OLD_DUMP)).toBe(true)
      expect(outcome.backupEntries).toEqual([testCase.dumpFileName])
    },
  )

  test('rejects an empty size-probe response before staging so a zero-byte zero-exit stream cannot replace the previous dump', async () => {
    const pgCase = BUILDERS[0]!
    const outcome = await runPreBackup(pgCase, {
      sizeProbeStdout: '',
      truncateReaderZeroExitAfterBytes: 0,
    })

    expect(outcome.error).toBeInstanceOf(Error)
    expect((outcome.error as Error).message).toContain('Invalid dump size')
    expect(outcome.events).toEqual([])
    expect(outcome.renameCalled).toBe(false)
    expect(outcome.finalTargetBytes?.equals(OLD_DUMP)).toBe(true)
    expect(outcome.backupEntries).toEqual([pgCase.dumpFileName])
  })

  test('reads the dump from inside the subcontainer when /tmp is an absolute symlink', async () => {
    const pgCase = BUILDERS[0]!
    const outcome = await runPreBackup(pgCase, {
      beforeRun: ({ rootfs, tmpDir }) => {
        const target = '/srv/start-sdk-dump-src'
        mkdirSync(join(rootfs, target), { recursive: true })
        rmSync(tmpDir, { recursive: true, force: true })
        symlinkSync(target, tmpDir)
      },
    })

    expect(outcome.error).toBeNull()
    expect(outcome.finalTargetBytes?.equals(NEW_DUMP)).toBe(true)
    expect(outcome.backupEntries).toEqual([pgCase.dumpFileName])
  })

  test('preserves pre-existing sibling files matching staging name patterns on both failure and success', async () => {
    const pgCase = BUILDERS[0]!
    const legacyData = Buffer.from('package-owned-legacy-file')
    const matchingData = Buffer.from('package-owned-matching-tmp-file')
    const legacyName = `.${pgCase.dumpFileName}.tmp`
    const matchingName = `.${pgCase.dumpFileName}.0123456789abcdef01234567.tmp`

    for (const failSync of [true, false]) {
      let legacyAfter: Buffer | null = null
      let matchingAfter: Buffer | null = null
      const outcome = await runPreBackup(pgCase, {
        failSync,
        beforeRun: ({ backupDir }) => {
          writeFileSync(join(backupDir, legacyName), legacyData)
          writeFileSync(join(backupDir, matchingName), matchingData)
        },
        afterRun: ({ backupDir }) => {
          legacyAfter = readFileSync(join(backupDir, legacyName))
          matchingAfter = readFileSync(join(backupDir, matchingName))
        },
      })
      expect(outcome.backupEntries).toEqual(
        [legacyName, matchingName, pgCase.dumpFileName].sort(),
      )
      expect(legacyAfter).toEqual(legacyData)
      expect(matchingAfter).toEqual(matchingData)
      if (failSync) {
        expect(outcome.error).toBeInstanceOf(Error)
        expect(outcome.finalTargetBytes?.equals(OLD_DUMP)).toBe(true)
      } else {
        expect(outcome.error).toBeNull()
        expect(outcome.finalTargetBytes?.equals(NEW_DUMP)).toBe(true)
      }
    }
  })

  test('fails with EISDIR and leaves the directory empty if a directory occupies the live dump path', async () => {
    const pgCase = BUILDERS[0]!
    let dirEntriesAfter: string[] | null = null
    const outcome = await runPreBackup(pgCase, {
      seedTarget: false,
      beforeRun: ({ backupDir }) => {
        mkdirSync(join(backupDir, pgCase.dumpFileName))
      },
      afterRun: ({ backupDir }) => {
        dirEntriesAfter = readdirSync(join(backupDir, pgCase.dumpFileName))
      },
    })

    expect(outcome.error).toMatchObject({ code: 'EISDIR' })
    expect(outcome.targetStat?.isDirectory()).toBe(true)
    expect(outcome.backupEntries).toEqual([pgCase.dumpFileName])
    expect(dirEntriesAfter).toEqual([])
  })

  test('replaces a symlink-to-directory at the live dump path rather than moving the dump inside it', async () => {
    const pgCase = BUILDERS[0]!
    let otherDirEntriesAfter: string[] | null = null
    const outcome = await runPreBackup(pgCase, {
      seedTarget: false,
      beforeRun: ({ backupDir }) => {
        const otherDir = join(backupDir, 'other-dir')
        mkdirSync(otherDir)
        symlinkSync(otherDir, join(backupDir, pgCase.dumpFileName))
      },
      afterRun: ({ backupDir }) => {
        otherDirEntriesAfter = readdirSync(join(backupDir, 'other-dir'))
      },
    })

    expect(outcome.error).toBeNull()
    expect(outcome.targetStat?.isFile()).toBe(true)
    expect(outcome.finalTargetBytes?.equals(NEW_DUMP)).toBe(true)
    expect(outcome.backupEntries).toEqual([pgCase.dumpFileName, 'other-dir'])
    expect(otherDirEntriesAfter).toEqual([])
  })

  test('preserves the previous dump and cleans up the staged file on a pre-replacement rename failure', async () => {
    const pgCase = BUILDERS[0]!
    const outcome = await runPreBackup(pgCase, { failRename: true })

    expect(outcome.error).toBeInstanceOf(Error)
    expect((outcome.error as Error).message).toContain(
      'simulated pre-replacement rename failure',
    )
    expect(outcome.finalTargetBytes?.equals(OLD_DUMP)).toBe(true)
    expect(outcome.backupEntries).toEqual([pgCase.dumpFileName])
  })

  test('surfaces the pre-replacement rename error even if temp cleanup fails', async () => {
    const pgCase = BUILDERS[0]!
    const outcome = await runPreBackup(pgCase, {
      failRename: true,
      failCleanupRm: true,
    })

    expect(outcome.error).toBeInstanceOf(Error)
    expect((outcome.error as Error).message).toContain(
      'simulated pre-replacement rename failure',
    )
    expect(outcome.finalTargetBytes?.equals(OLD_DUMP)).toBe(true)
  })

  test('propagates a post-rename directory fsync error while keeping the newly replaced dump visible', async () => {
    const pgCase = BUILDERS[0]!
    const outcome = await runPreBackup(pgCase, { failDirSync: true })

    expect(outcome.error).toBeInstanceOf(Error)
    expect((outcome.error as Error).message).toContain('directory fsync failed')
    expect(outcome.finalTargetBytes?.equals(NEW_DUMP)).toBe(true)
    expect(outcome.backupEntries).toEqual([pgCase.dumpFileName])
  })

  test.each(['../escape', 'sub/dir'])(
    'rejects a database name containing path separators (%s) at builder construction before any subcontainer command runs',
    database => {
      const withTempSpy = jest.spyOn(SubContainer, 'withTemp')
      expect(() =>
        Backups.withPgDump<TestManifest>({
          imageId: 'db',
          dbVolume: 'db',
          mountpoint: '/var/lib/postgresql',
          pgdataPath: '/data',
          database,
          user: 'postgres',
          password: null,
        }),
      ).toThrow('Invalid backup dump filename')
      expect(() =>
        Backups.withMysqlDump<TestManifest>({
          imageId: 'db',
          dbVolume: 'db',
          datadir: '/var/lib/mysql',
          database,
          user: 'app',
          password: 'pw',
        }),
      ).toThrow('Invalid backup dump filename')
      expect(() =>
        Backups.withMariadbDump<TestManifest>({
          imageId: 'db',
          dbVolume: 'db',
          datadir: '/var/lib/mysql',
          database,
          user: 'app',
          password: 'pw',
        }),
      ).toThrow('Invalid backup dump filename')
      expect(withTempSpy).not.toHaveBeenCalled()
    },
  )

  test('rejects without uncaughtException, closes the staging handle, and skips rename when reader stderr errors', async () => {
    const pgCase = BUILDERS[0]!
    const uncaught: unknown[] = []
    const onUncaught = (err: unknown) => {
      uncaught.push(err)
    }
    process.on('uncaughtException', onUncaught)
    try {
      const outcome = await runPreBackup(pgCase, { failReaderStderr: true })

      expect(uncaught).toEqual([])
      expect(outcome.error).toBeInstanceOf(Error)
      expect((outcome.error as Error).message).toContain(
        'simulated stderr pipe failure',
      )
      expect(outcome.events).toEqual(['open-wx', 'file-close'])
      expect(outcome.renameCalled).toBe(false)
      expect(outcome.finalTargetBytes?.equals(OLD_DUMP)).toBe(true)
      expect(outcome.backupEntries).toEqual([pgCase.dumpFileName])
    } finally {
      process.off('uncaughtException', onUncaught)
    }
  })
})
