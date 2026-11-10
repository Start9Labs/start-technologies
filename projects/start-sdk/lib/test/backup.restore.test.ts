import { EventEmitter } from 'events'
import * as child_process from 'child_process'
import * as fs from 'fs/promises'
import { Backups } from '../backup/Backups'

jest.mock('child_process', () => ({
  ...jest.requireActual('child_process'),
  spawn: jest.fn(),
}))

jest.mock('fs/promises', () => ({
  ...jest.requireActual('fs/promises'),
  access: jest.fn(),
  mkdir: jest.fn(),
  readFile: jest.fn(async () => {
    throw new Error('ENOENT')
  }),
}))

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
  beforeEach(() => {
    jest.clearAllMocks()
    jest.mocked(child_process.spawn).mockImplementation(fakeRsync as never)
  })

  it('restores the volumes a backup contains and skips the ones it lacks', async () => {
    jest
      .mocked(fs.access)
      .mockImplementation(async path =>
        String(path).includes('/volumes/public/')
          ? Promise.reject(new Error('ENOENT'))
          : undefined,
      )

    await Backups.ofVolumes<any>('main', 'public').restoreBackup(effects)

    const sources = jest
      .mocked(child_process.spawn)
      .mock.calls.map(([, args]) => (args as string[]).at(-2))
    expect(sources).toEqual(['/media/startos/backup/volumes/main/'])
  })
})
