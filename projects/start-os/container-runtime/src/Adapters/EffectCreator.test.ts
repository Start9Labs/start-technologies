import { EventEmitter } from 'events'
import * as net from 'net'
import { CallbackHolder } from '../Models/CallbackHolder'
import { makeEffects } from './EffectCreator'

jest.mock('net', () => ({ createConnection: jest.fn() }))

describe('makeEffects action event identity', () => {
  let writes: string[]

  beforeEach(() => {
    writes = []
    jest
      .mocked(net.createConnection)
      .mockImplementation((...args: unknown[]) => {
        const onConnect = args[1] as () => void
        const socket = Object.assign(new EventEmitter(), {
          write: jest.fn((data: string) => {
            writes.push(data)
            queueMicrotask(() => {
              socket.emit('data', Buffer.from('{"result":null}\n'))
            })
            return true
          }),
          end: jest.fn(),
        })
        queueMicrotask(onConnect)
        return socket as unknown as net.Socket
      })
  })

  describe.each([
    ['getInput', 'action.get-input'],
    ['run', 'action.run'],
  ] as const)('%s', (effectMethod, rpcMethod) => {
    test.each([
      {
        contextId: null,
        override: 'generated-event',
        expected: 'generated-event',
      },
      {
        contextId: 'caller-event',
        override: undefined,
        expected: 'caller-event',
      },
      {
        contextId: 'caller-event',
        override: 'explicit-event',
        expected: 'explicit-event',
      },
      { contextId: null, override: undefined, expected: undefined },
    ])(
      'serializes $expected with context $contextId and override $override',
      async ({ contextId, override, expected }) => {
        const context = { eventId: contextId }
        const effects = makeEffects(context)
        const options = {
          packageId: 'target',
          actionId: 'configure',
          ...(override === undefined ? {} : { eventId: override }),
        }

        await effects.action[effectMethod](options)

        expect(writes).toHaveLength(1)
        expect(writes[0].endsWith('\n')).toBe(true)
        expect(JSON.parse(writes[0])).toEqual({
          id: expect.any(Number),
          method: rpcMethod,
          params: {
            packageId: 'target',
            actionId: 'configure',
            ...(expected === undefined ? {} : { eventId: expected }),
          },
        })
        expect(context.eventId).toBe(contextId)
        expect(effects.eventId).toBe(contextId)
      },
    )
  })

  test('action overrides leave unrelated RPCs and callback ownership in the caller context', async () => {
    const callbacks = new CallbackHolder()
    const constRetry = jest.fn()
    const context = { eventId: 'caller-event', callbacks, constRetry }
    const effects = makeEffects(context)
    const onLeave = jest.fn()
    effects.onLeaveContext(onLeave)

    await effects.action.getInput({
      actionId: 'configure',
      eventId: 'action-event',
    })
    await effects.action.run({
      actionId: 'configure',
      eventId: 'action-event',
    })
    await effects.action.clear({ except: [] })
    await effects.getOsIp()

    expect(writes.map(data => JSON.parse(data).params.eventId)).toEqual([
      'action-event',
      'action-event',
      'caller-event',
      'caller-event',
    ])
    expect(context).toEqual({ eventId: 'caller-event', callbacks, constRetry })
    expect(effects.eventId).toBe('caller-event')
    expect(effects.constRetry).toBe(constRetry)
    expect(effects.isInContext).toBe(true)
    expect(onLeave).not.toHaveBeenCalled()
    callbacks.leaveContext()
    expect(onLeave).toHaveBeenCalledTimes(1)
    expect(effects.isInContext).toBe(false)
    expect(effects.constRetry).toBeUndefined()
  })
})
