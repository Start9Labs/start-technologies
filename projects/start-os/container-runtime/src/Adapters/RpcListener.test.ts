import { EventEmitter } from 'events'
import * as net from 'net'
import { System } from '../Interfaces/System'
import { RpcListener } from './RpcListener'

jest.mock('net', () => ({
  createServer: jest.fn(),
  createConnection: jest.fn(),
}))

jest.mock('fs', () => ({
  ...jest.requireActual('fs'),
  existsSync: jest.fn(() => true),
  rmSync: jest.fn(),
  mkdirSync: jest.fn(),
}))

function system(): jest.Mocked<System> {
  return {
    init: jest.fn(async (..._args: Parameters<System['init']>) => {}),
    exit: jest.fn(async (..._args: Parameters<System['exit']>) => {}),
    start: jest.fn(async (..._args: Parameters<System['start']>) => {}),
    stop: jest.fn(async () => {}),
    createBackup: jest.fn(
      async (..._args: Parameters<System['createBackup']>) => {},
    ),
    runAction: jest.fn(
      async (
        ..._args: Parameters<System['runAction']>
      ): ReturnType<System['runAction']> => null,
    ),
    getActionInput: jest.fn(
      async (
        ..._args: Parameters<System['getActionInput']>
      ): ReturnType<System['getActionInput']> => null,
    ),
  }
}

beforeEach(() => {
  jest.mocked(net.createServer).mockReturnValue(
    Object.assign(new EventEmitter(), {
      listen: jest.fn(),
    }) as unknown as net.Server,
  )
})

test('a backup-only runtime executes the backup without init, main or exit hooks', async () => {
  const abi = system()
  const listener = new RpcListener({ system: async () => abi })
  expect(
    await listener['dealWithInput']({
      id: 1,
      method: 'init-backup',
      params: { id: 'event' },
    }),
  ).toEqual({
    jsonrpc: '2.0',
    id: 1,
    result: null,
  })
  expect(
    await listener['dealWithInput']({
      id: 2,
      method: 'execute',
      params: { id: 'event', procedure: '/backup/create', input: null },
    }),
  ).toEqual({ jsonrpc: '2.0', id: 2, result: null })
  expect(abi.createBackup).toHaveBeenCalledTimes(1)
  expect(
    await listener['dealWithInput']({
      id: 3,
      method: 'exit',
      params: { id: 'event', target: null },
    }),
  ).toEqual({
    jsonrpc: '2.0',
    id: 3,
    result: null,
  })
  expect(abi.init).not.toHaveBeenCalled()
  expect(abi.start).not.toHaveBeenCalled()
  expect(abi.exit).not.toHaveBeenCalled()
  expect(listener.shouldExit).toBe(true)
})

test('normal initialization and exit still run service hooks', async () => {
  const abi = system()
  const listener = new RpcListener({ system: async () => abi })
  await listener['dealWithInput']({
    id: 1,
    method: 'init',
    params: { id: 'event', kind: 'restore' },
  })
  expect(abi.init).toHaveBeenCalledWith(expect.any(Object), 'restore')
  await listener['dealWithInput']({
    id: 2,
    method: 'exit',
    params: { id: 'event', target: null },
  })
  expect(abi.exit).toHaveBeenCalledWith(expect.any(Object), null)
  expect(listener.shouldExit).toBe(true)
})

test('a failed backup still exits without running uninitialized service hooks', async () => {
  const abi = system()
  abi.createBackup.mockRejectedValue(new Error('backup failed'))
  const listener = new RpcListener({ system: async () => abi })
  await listener['dealWithInput']({
    id: 1,
    method: 'init-backup',
    params: { id: 'event' },
  })
  const result = await listener['dealWithInput']({
    id: 2,
    method: 'execute',
    params: { id: 'event', procedure: '/backup/create', input: null },
  })
  expect(result).toMatchObject({
    error: { code: 59, data: { details: 'Error: backup failed' } },
  })
  await listener['dealWithInput']({
    id: 3,
    method: 'exit',
    params: { id: 'event', target: null },
  })
  expect(abi.exit).not.toHaveBeenCalled()
  expect(listener.shouldExit).toBe(true)
})
