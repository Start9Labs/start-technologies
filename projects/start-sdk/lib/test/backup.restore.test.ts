import { EventEmitter } from 'events'
import * as child_process from 'child_process'
import * as fs from 'fs/promises'
import * as os from 'os'
import * as path from 'path'
import { FullProgressTracker } from '@start9labs/start-core/util/FullProgressTracker'
import { Backups } from '../backup/Backups'

jest.mock('child_process', () => ({
  ...jest.requireActual('child_process'),
  spawn: jest.fn(),
}))

jest.mock('fs/promises', () => ({
  ...jest.requireActual('fs/promises'),
  lstat: jest.fn(),
  mkdir: jest.fn(),
  readFile: jest.fn(async () => {
    throw new Error('ENOENT')
  }),
}))

const actualFs = jest.requireActual('fs/promises') as typeof fs

function fakeRsync() {
  const proc = Object.assign(new EventEmitter(), {
    pid: 1,
    stdout: new EventEmitter(),
    stderr: new EventEmitter(),
  })
  setImmediate(() => proc.emit('exit', 0))
  return proc
}

const effects = {
  setInitProgress: jest.fn(async () => null),
  setDataVersion: jest.fn(async () => null),
} as never

describe('restoreBackup', () => {
  let directory: string

  beforeEach(async () => {
    jest.clearAllMocks()
    directory = await actualFs.mkdtemp(path.join(os.tmpdir(), 'sdk-restore-'))
    jest.mocked(fs.lstat).mockImplementation(actualFs.lstat)
    jest.mocked(child_process.spawn).mockImplementation(fakeRsync as never)
  })

  afterEach(async () => {
    await actualFs.rm(directory, { recursive: true, force: true })
  })

  it('restores the volumes a backup contains and completes skipped phases', async () => {
    jest.mocked(fs.lstat).mockImplementation(async source => {
      if (String(source).includes('/volumes/public/')) {
        throw Object.assign(new Error('missing source'), { code: 'ENOENT' })
      }
      return actualFs.lstat(directory)
    })
    const progress = new FullProgressTracker()
    const postRestore = jest.fn(async () => {})

    await Backups.ofVolumes<any>('main', 'public')
      .setPostRestore(postRestore)
      .restoreBackup(effects, progress)

    const sources = jest
      .mocked(child_process.spawn)
      .mock.calls.map(([, args]) => (args as string[]).at(-2))
    expect(sources).toEqual(['/media/startos/backup/volumes/main/'])
    expect(
      progress.getPhase('restore:/media/startos/volumes/public/')?.snapshot(),
    ).toBe(true)
    expect(postRestore).toHaveBeenCalledTimes(1)
  })

  it.each(['EIO', 'EACCES', 'ENOTCONN'])(
    'rejects %s instead of silently skipping source data',
    async code => {
      const error = Object.assign(new Error('source lookup failed'), { code })
      jest.mocked(fs.lstat).mockRejectedValueOnce(error)
      const postRestore = jest.fn(async () => {})

      await expect(
        Backups.ofVolumes<any>('main')
          .setPostRestore(postRestore)
          .restoreBackup(effects),
      ).rejects.toBe(error)

      expect(child_process.spawn).not.toHaveBeenCalled()
      expect(postRestore).not.toHaveBeenCalled()
    },
  )

  it('restores a dangling symlink source without requiring its target', async () => {
    const source = path.join(directory, 'link')
    await actualFs.symlink('missing-target', source)

    await Backups.ofSyncs<any>({
      dataPath: '/media/startos/volumes/main/link',
      backupPath: source as `/media/startos/backup/${string}`,
    }).restoreBackup(effects)

    expect(child_process.spawn).toHaveBeenCalledWith(
      'rsync',
      expect.arrayContaining([source, '/media/startos/volumes/main/link']),
      { detached: true },
    )
  })
})
