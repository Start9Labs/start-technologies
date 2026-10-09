import { setupManifest, StartSdk } from '@start9labs/start-sdk'
import { randomUUID } from 'crypto'
import { EventEmitter } from 'events'
import * as net from 'net'
import { CallbackHolder } from '../Models/CallbackHolder'
import { makeEffects } from './EffectCreator'

jest.mock('net', () => ({ createConnection: jest.fn() }))

const sdk = StartSdk.of()
  .withManifest(
    setupManifest({
      id: 'caller',
      title: '',
      license: '',
      packageRepo: '',
      upstreamRepo: '',
      marketingUrl: '',
      donationUrl: null,
      description: { short: '', long: '' },
      images: {},
      volumes: [],
    }),
  )
  .build(true)

const metadata = {
  name: 'Attach',
  description: '',
  warning: null,
  allowedStatuses: 'any' as const,
  group: null,
  visibility: 'hidden' as const,
  access: 'public' as const,
}

type Input = { hostId: string; address: string }
type Request = {
  id: number
  method: string
  params: {
    packageId?: string
    actionId?: string
    eventId?: string
    prefill?: Partial<Input> | null
    input?: Input
  }
}

function mockHostTransport(dispatch: (request: Request) => Promise<unknown>) {
  const requests: Request[] = []
  jest.mocked(net.createConnection).mockImplementation((...args: unknown[]) => {
    const onConnect = args[1] as () => void
    const socket = Object.assign(new EventEmitter(), {
      write: jest.fn((data: string) => {
        expect(data.endsWith('\n')).toBe(true)
        const request: Request = JSON.parse(data)
        requests.push(request)
        void dispatch(request).then(
          result =>
            socket.emit('data', Buffer.from(JSON.stringify({ result }) + '\n')),
          error => socket.emit('error', error),
        )
        return true
      }),
      end: jest.fn(),
    })
    queueMicrotask(onConnect)
    return socket as unknown as net.Socket
  })
  return requests
}

function attachAction(ran: jest.Mock) {
  return sdk.Action.withInput(
    'attach',
    metadata,
    async ({ prefill }) =>
      sdk.InputSpec.of({
        hostId: sdk.Value.hidden<string>(),
        address: sdk.Value.select({
          name: 'Address',
          default: 'new',
          values: {
            new: 'New',
            [`${(prefill as Partial<Input> | null)?.hostId}-0`]: 'Existing',
          },
        }),
      }),
    async () => null,
    async ({ input, caller }) => {
      ran(input, caller)
      return null
    },
  )
}

describe('makeEffects event ID serialization', () => {
  let requests: Request[]

  beforeEach(() => {
    requests = mockHostTransport(async () => null)
  })

  test.each([
    [null, 'request-event'],
    ['caller-event', 'request-event'],
  ])(
    'uses supplied action IDs without mutating caller context %s',
    async (eventId, expectedEventId) => {
      const callbacks = new CallbackHolder()
      const constRetry = jest.fn()
      const context = Object.freeze({ eventId, callbacks, constRetry })
      const effects = makeEffects(context)
      const onLeave = jest.fn()
      effects.onLeaveContext(onLeave)
      const getInput = Object.freeze({
        packageId: 'target',
        actionId: 'attach',
        eventId: 'request-event',
        prefill: { hostId: 'peer' },
      })
      const run = Object.freeze({
        packageId: 'target',
        actionId: 'attach',
        eventId: 'request-event',
        input: { hostId: 'peer', address: 'peer-0' },
      })

      await effects.action.getInput(getInput)
      await effects.action.run(run)

      expect(requests).toEqual([
        {
          id: expect.any(Number),
          method: 'action.get-input',
          params: { ...getInput, eventId: expectedEventId },
        },
        {
          id: expect.any(Number),
          method: 'action.run',
          params: { ...run, eventId: expectedEventId },
        },
      ])
      expect(getInput.eventId).toBe('request-event')
      expect(run.eventId).toBe('request-event')
      expect(context).toEqual({ eventId, callbacks, constRetry })
      expect(effects.eventId).toBe(eventId)
      expect(effects.constRetry).toBe(constRetry)
      expect(effects.isInContext).toBe(true)
      expect(onLeave).not.toHaveBeenCalled()
    },
  )

  test.each([
    [null, {}],
    ['caller-event', { eventId: 'caller-event' }],
  ])(
    'falls back to caller event %s when action and non-action IDs are absent',
    async (eventId, expectedEventParams) => {
      const effects = makeEffects({ eventId })
      await effects.action.getInput({ actionId: 'attach' })
      await effects.action.run({ actionId: 'attach', input: {} })
      await effects.action.clear({ except: [] })
      await effects.getStatus({ packageId: 'target' })
      expect(requests).toEqual([
        {
          id: expect.any(Number),
          method: 'action.get-input',
          params: { actionId: 'attach', ...expectedEventParams },
        },
        {
          id: expect.any(Number),
          method: 'action.run',
          params: { actionId: 'attach', input: {}, ...expectedEventParams },
        },
        {
          id: expect.any(Number),
          method: 'action.clear',
          params: { except: [], ...expectedEventParams },
        },
        {
          id: expect.any(Number),
          method: 'get-status',
          params: {
            packageId: 'target',
            callback: null,
            ...expectedEventParams,
          },
        },
      ])
      expect(effects.eventId).toBe(eventId)
    },
  )

  test.each([
    [null, 'request-event'],
    ['caller-event', 'request-event'],
  ])(
    'uses supplied effect IDs without mutating options or caller event %s',
    async (eventId, expectedEventId) => {
      const effects = makeEffects({ eventId })
      const options = Object.freeze({ except: [], eventId: 'request-event' })
      const statusOptions = Object.freeze({
        packageId: 'target',
        eventId: 'request-event',
      })
      await effects.action.clear(options)
      await effects.getStatus(statusOptions)
      expect(requests).toEqual([
        {
          id: expect.any(Number),
          method: 'action.clear',
          params: { ...options, eventId: expectedEventId },
        },
        {
          id: expect.any(Number),
          method: 'get-status',
          params: {
            ...statusOptions,
            callback: null,
            eventId: expectedEventId,
          },
        },
      ])
      expect(options.eventId).toBe('request-event')
      expect(statusOptions.eventId).toBe('request-event')
      expect(effects.eventId).toBe(eventId)
    },
  )
})

describe('sdk.action.run through makeEffects', () => {
  let requests: Request[]
  let ran: jest.Mock

  beforeEach(() => {
    ran = jest.fn()
    const attach = attachAction(ran)
    const reset = sdk.Action.withoutInput('reset', metadata, async () => {
      ran()
      return null
    })
    const dispatch = async ({ method, params }: Request) => {
      const effects = makeEffects({
        eventId: params.eventId ?? randomUUID(),
      })
      switch (method) {
        case 'action.get-input':
          expect(params.packageId).toBe('target')
          expect(params.actionId).toBe('attach')
          return attach.getInput({
            effects,
            prefill: params.prefill ?? null,
            caller: 'caller',
          })
        case 'action.run':
          expect(params.packageId).toBe('target')
          return params.actionId === 'reset'
            ? reset.run({ effects, input: {}, caller: 'caller' })
            : attach.run({
                effects,
                input: params.input!,
                caller: 'caller',
              })
        case 'get-os-ip':
          return '127.0.0.1'
        default:
          throw new Error(`Unexpected RPC ${method}`)
      }
    }
    requests = mockHostTransport(dispatch)
  })

  const run = (effects: ReturnType<typeof makeEffects>, hostId: string) =>
    sdk.action.run({
      effects,
      packageId: 'target',
      actionId: 'attach',
      prefill: { hostId },
      input: ({ spec, value }) => {
        expect(
          Object.keys((spec.address as { values: object }).values),
        ).toEqual(['new', `${hostId}-0`])
        expect(value).toBeNull()
        return { hostId, address: `${hostId}-0` }
      },
    })

  test('pairs the serialized form and submission without changing the caller context', async () => {
    const callbacks = new CallbackHolder()
    const constRetry = jest.fn()
    const context = { eventId: null, callbacks, constRetry }
    const effects = makeEffects(context)
    const onLeave = jest.fn()
    effects.onLeaveContext(onLeave)

    await expect(run(effects, 'peer')).resolves.toBeNull()
    expect(requests.map(request => request.method)).toEqual([
      'action.get-input',
      'action.run',
    ])
    const eventId = requests[0].params.eventId
    expect(eventId).toEqual(expect.any(String))
    expect(eventId).not.toBe('')
    expect(requests[1].params.eventId).toBe(eventId)
    expect(ran).toHaveBeenCalledWith(
      { hostId: 'peer', address: 'peer-0' },
      'caller',
    )

    await effects.getOsIp()
    expect(requests[2].params).not.toHaveProperty('eventId')
    expect(context).toEqual({ eventId: null, callbacks, constRetry })
    expect(effects.eventId).toBeNull()
    expect(effects.constRetry).toBe(constRetry)
    expect(effects.isInContext).toBe(true)
    expect(onLeave).not.toHaveBeenCalled()
    callbacks.leaveContext()
    expect(onLeave).toHaveBeenCalledTimes(1)
    expect(effects.isInContext).toBe(false)
    expect(effects.constRetry).toBeUndefined()
  })

  test.each(['sequential', 'concurrent'])(
    '%s calls from one null-event caller keep independent dynamic forms',
    async mode => {
      const effects = makeEffects({ eventId: null })
      if (mode === 'concurrent') {
        await Promise.all([run(effects, 'first'), run(effects, 'second')])
      } else {
        await run(effects, 'first')
        await run(effects, 'second')
      }

      const opened = requests.filter(
        request => request.method === 'action.get-input',
      )
      const submitted = requests.filter(
        request => request.method === 'action.run',
      )
      const eventIds = opened.map(request => request.params.eventId)
      expect(eventIds).toEqual([expect.any(String), expect.any(String)])
      expect(new Set(eventIds).size).toBe(2)
      for (const request of submitted) {
        expect(request.params.eventId).toBe(
          opened.find(
            form =>
              form.params.prefill?.hostId === request.params.input?.hostId,
          )?.params.eventId,
        )
      }
      expect(submitted).toHaveLength(2)
      expect(ran).toHaveBeenCalledTimes(2)
      for (const hostId of ['first', 'second']) {
        expect(ran).toHaveBeenCalledWith(
          { hostId, address: `${hostId}-0` },
          'caller',
        )
      }
      expect(effects.eventId).toBeNull()
    },
  )

  test('retains an existing caller event ID on the wire', async () => {
    const effects = makeEffects({ eventId: 'caller-event' })
    await run(effects, 'peer')
    expect(requests.map(request => request.params.eventId)).toEqual([
      'caller-event',
      'caller-event',
    ])
    expect(effects.eventId).toBe('caller-event')
    expect(ran).toHaveBeenCalledTimes(1)
  })

  test('rejects a submission outside the dynamic form opened on the wire', async () => {
    await expect(
      sdk.action.run({
        effects: makeEffects({ eventId: null }),
        packageId: 'target',
        actionId: 'attach',
        prefill: { hostId: 'peer' },
        input: () => ({ hostId: 'peer', address: 'other-0' }),
      }),
    ).rejects.toThrow()
    expect(requests).toHaveLength(2)
    expect(requests[0].params.eventId).toEqual(expect.any(String))
    expect(requests[1].params.eventId).toBe(requests[0].params.eventId)
    expect(ran).not.toHaveBeenCalled()
  })

  test.each([null, 'caller-event'])(
    'no-input calls preserve caller event %s',
    async eventId => {
      const effects = makeEffects({ eventId })
      await expect(
        sdk.action.run({ effects, packageId: 'target', actionId: 'reset' }),
      ).resolves.toBeNull()
      expect(requests).toEqual([
        {
          id: expect.any(Number),
          method: 'action.run',
          params: {
            packageId: 'target',
            actionId: 'reset',
            ...(eventId === null ? {} : { eventId }),
          },
        },
      ])
      expect(ran).toHaveBeenCalledTimes(1)
      expect(effects.eventId).toBe(eventId)
    },
  )
})
