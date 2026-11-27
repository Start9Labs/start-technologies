import { EventEmitter } from 'node:events'
import * as childProcess from 'child_process'
import { T } from '@start9labs/start-core'
import { Backups, parseRsyncTransferredBytes } from '../backup/Backups'

jest.mock('child_process', () => ({
  ...jest.requireActual('child_process'),
  spawn: jest.fn(),
}))
jest.mock('fs/promises', () => ({
  ...jest.requireActual('fs/promises'),
  mkdir: jest.fn(),
}))

describe('scheduled backup rsync statistics', () => {
  test('parses transferred bytes from structured stats', () => {
    expect(
      parseRsyncTransferredBytes(
        'Number of files: 42\nTotal transferred file size: 1,234,567 bytes\n',
      ),
    ).toBe(1_234_567)
  })

  test('returns unknown when structured stats are unavailable', () => {
    expect(
      parseRsyncTransferredBytes('custom backup hook completed\n'),
    ).toBeNull()
  })

  test('handles a stats line assembled from output chunks', () => {
    const chunks = ['Total transferred file ', 'size: 99 bytes\n']
    expect(parseRsyncTransferredBytes(chunks.join(''))).toBe(99)
  })
})

test('a reused backup configuration measures each run independently', async () => {
  const stdout = ['', 'Total transferred file size: 42 bytes\n']
  jest.mocked(childProcess.spawn).mockImplementation(() => {
    const child = Object.assign(new EventEmitter(), {
      stdout: new EventEmitter(),
      stderr: new EventEmitter(),
      pid: 123,
    })
    setImmediate(() => {
      child.stdout.emit('data', stdout.shift())
      child.emit('close', 0)
    })
    return child as childProcess.ChildProcessWithoutNullStreams
  })
  const effects = {
    setBackupProgress: async () => {},
    getDataVersion: async () => null,
  } as unknown as T.Effects
  const backups = Backups.ofVolumes<T.SDKManifest>('data')

  await expect(backups.createBackup(effects)).resolves.toEqual({
    changedBytes: null,
  })
  await expect(backups.createBackup(effects)).resolves.toEqual({
    changedBytes: 42,
  })
})
