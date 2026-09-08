import * as childProcess from 'child_process'
import * as fs from 'fs/promises'
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

type Engine = 'mysql' | 'mariadb'

const result = { exitCode: 0, stdout: Buffer.alloc(0), stderr: Buffer.alloc(0) }

function fakeSubcontainer(engine: Engine) {
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

describe.each<Engine>(['mysql', 'mariadb'])('%s dump restore', engine => {
  beforeEach(() => {
    jest.clearAllMocks()
    jest.useFakeTimers()
  })
  afterEach(() => {
    jest.clearAllTimers()
    jest.useRealTimers()
    jest.restoreAllMocks()
  })

  test('binds a leading-hyphen database as the import database', async () => {
    const fake = fakeSubcontainer(engine)
    jest.spyOn(SubContainer, 'withTemp').mockImplementationOnce((async (
      ...args: unknown[]
    ) => {
      const fn = args[4] as (sub: typeof fake) => Promise<void>
      await fn(fake)
    }) as typeof SubContainer.withTemp)

    const backups = Backups.withMysqlDump<any>({
      imageId: 'database',
      dbVolume: 'database',
      datadir: '/var/lib/mysql',
      database: '--help',
      user: 'app',
      password: 'secret',
      engine,
    })
    const restore = (backups as unknown as { postRestore: BackupHook })
      .postRestore

    await restore({} as never, {} as never)

    expect(fake.execFail).toHaveBeenCalledWith(
      [
        'sh',
        '-c',
        `exec ${engine === 'mysql' ? 'mysql' : 'mariadb'} -u root --database="$1" < "$2"`,
        'sh',
        '--help',
        '/tmp/db.sql',
      ],
      expect.objectContaining({ user: 'root', timeout: null }),
    )
    expect(childProcess.execFile).toHaveBeenCalled()
    expect(fs.mkdir).toHaveBeenCalled()
  })
})
