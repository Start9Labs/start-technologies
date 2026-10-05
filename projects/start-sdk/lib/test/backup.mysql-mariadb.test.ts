import { Backups } from '../backup/Backups'
import type { BackupHook } from '../backup/Backups'
import { SubContainer } from '../util/SubContainer'

jest.mock('child_process', () => ({
  ...jest.requireActual('child_process'),
  execFile: jest.fn(
    (
      _file: string,
      _args: string[],
      callback: (error: null, stdout: string, stderr: string) => void,
    ) => callback(null, '', ''),
  ),
}))

jest.mock('fs/promises', () => ({
  ...jest.requireActual('fs/promises'),
  mkdir: jest.fn(),
}))

const result = { exitCode: 0, stdout: Buffer.alloc(0), stderr: Buffer.alloc(0) }

function fakeSubcontainer() {
  let stopServer: (() => void) | undefined
  const server = new Promise<typeof result>(resolve => {
    stopServer = () => resolve(result)
  })
  const exec = jest.fn(async (command: string[]) => {
    if (command[0] === 'mariadbd') return server
    if (command[0] === 'pkill') stopServer?.()
    return result
  })
  return {
    rootfs: '/tmp/start-sdk-backup-test',
    exec,
    execFail: jest.fn(async () => result),
  }
}

async function expectLeadingHyphenImport(
  backups: Backups<any>,
  client: 'mysql' | 'mariadb',
) {
  const fake = fakeSubcontainer()
  jest.spyOn(SubContainer, 'withTemp').mockImplementationOnce((async (
    ...args: unknown[]
  ) => {
    const fn = args[4] as (sub: typeof fake) => Promise<void>
    await fn(fake)
  }) as typeof SubContainer.withTemp)

  const restore = (backups as unknown as { postRestore: BackupHook })
    .postRestore
  await restore({} as never, {} as never)

  expect(fake.execFail).toHaveBeenCalledWith(
    [
      'sh',
      '-c',
      `exec ${client} -u root --database="$1" < "$2"`,
      'sh',
      '--help',
      '/tmp/db.sql',
    ],
    expect.objectContaining({ user: 'root', timeout: null }),
  )
}

describe('MySQL and MariaDB dump restore', () => {
  beforeEach(() => {
    jest.clearAllMocks()
    jest.useFakeTimers()
  })

  afterEach(() => {
    jest.clearAllTimers()
    jest.useRealTimers()
    jest.restoreAllMocks()
  })

  test('keeps the MySQL engine discriminator for compatibility', () => {
    const config = {
      imageId: 'database',
      dbVolume: 'database',
      datadir: '/var/lib/mysql',
      database: 'app',
      user: 'app',
      password: 'secret',
    }
    const mysql = { ...config, engine: 'mysql' as const }
    const mariadb = { ...config }

    Backups.withMysqlDump<any>(mysql)
    Backups.withMariadbDump<any>(mariadb)
    // @ts-expect-error MariaDB configurations use withMariadbDump.
    Backups.withMysqlDump<any>({ ...mariadb, engine: 'mariadb' })
    // @ts-expect-error withMariadbDump selects the engine.
    Backups.withMariadbDump<any>({ ...mariadb, engine: 'mariadb' })
  })

  test('binds a leading-hyphen MySQL database as the import database', async () => {
    await expectLeadingHyphenImport(
      Backups.withMysqlDump<any>({
        imageId: 'database',
        dbVolume: 'database',
        datadir: '/var/lib/mysql',
        database: '--help',
        user: 'app',
        password: 'secret',
        engine: 'mysql',
      }),
      'mysql',
    )
  })

  test('binds a leading-hyphen MariaDB database as the import database', async () => {
    await expectLeadingHyphenImport(
      Backups.withMariadbDump<any>({
        imageId: 'database',
        dbVolume: 'database',
        datadir: '/var/lib/mysql',
        database: '--help',
        user: 'app',
        password: 'secret',
      }),
      'mariadb',
    )
  })
})
