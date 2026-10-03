import { EventEmitter } from 'events'
import * as net from 'net'
import { makeEffects } from './EffectCreator'

jest.mock('net', () => ({ createConnection: jest.fn() }))

describe('generated effects RPC boundary', () => {
  let requests: Array<{ method: string; params: Record<string, unknown> }>
  let reply: unknown
  let ended: jest.Mock

  beforeEach(() => {
    requests = []
    reply = { result: null }
    ended = jest.fn()
    jest
      .mocked(net.createConnection)
      .mockImplementation((...args: unknown[]) => {
        const connected = args[1] as () => void
        const socket = new EventEmitter()
        Object.assign(socket, {
          write: (data: string) => {
            requests.push(JSON.parse(data))
            const bytes = Buffer.from(JSON.stringify(reply) + '\n')
            queueMicrotask(() => {
              socket.emit('data', bytes.subarray(0, 3))
              socket.emit('data', bytes.subarray(3))
            })
          },
          end: ended,
        })
        queueMicrotask(connected)
        return socket as net.Socket
      })
  })

  test('serializes method params and preserves the event scope', async () => {
    const effects = makeEffects({ eventId: 'event' })
    await expect(
      effects.action.run({ actionId: 'configure' }),
    ).resolves.toBeNull()
    expect(requests).toEqual([
      {
        id: expect.any(Number),
        method: 'action.run',
        params: { actionId: 'configure', input: null, eventId: 'event' },
      },
    ])
    expect(ended).toHaveBeenCalled()
  })

  test('returns nullable IP and variable-length certificate results unchanged', async () => {
    const effects = makeEffects({ eventId: null })
    await expect(effects.getContainerIp({})).resolves.toBeNull()
    expect(requests[0].params).not.toHaveProperty('eventId')
    reply = { result: ['leaf', 'root'] }
    await expect(effects.getSslCertificate({ hostnames: [] })).resolves.toEqual(
      ['leaf', 'root'],
    )
  })

  test('retains host error details and method attribution', async () => {
    const error = jest.spyOn(console, 'error').mockImplementation(() => {})
    reply = {
      error: {
        code: 59,
        message: 'Runtime error',
        data: { details: 'broken' },
      },
    }
    try {
      await expect(makeEffects({ eventId: null }).restart()).rejects.toThrow(
        'Runtime error: broken@restart',
      )
    } finally {
      error.mockRestore()
    }
  })
})
