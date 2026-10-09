import { runAction } from '../actions'
import { Action } from '../actions/setupActions'
import { InputSpec } from '../actions/input/builder/inputSpec'
import { Value } from '../actions/input/builder/value'
import { Effects } from '../Effects'

const metadata = {
  name: 'Attach',
  description: '',
  warning: null,
  allowedStatuses: 'any' as const,
  group: null,
  visibility: 'hidden' as const,
  access: 'public' as const,
}

function callerEffects(
  action: ReturnType<typeof attachAction>,
  caller: string,
  eventId: string | null,
) {
  const targetEffects = (override?: string) =>
    ({
      eventId: override ?? eventId ?? crypto.randomUUID(),
    }) as Effects
  return Object.freeze({
    eventId,
    action: {
      getInput: jest.fn(
        async (options: Parameters<Effects['action']['getInput']>[0]) =>
          action.getInput({
            effects: targetEffects(options.eventId),
            prefill: (options.prefill ?? null) as {
              hostId?: string
              address?: string
            } | null,
            caller,
          }),
      ),
      run: jest.fn(async (options: Parameters<Effects['action']['run']>[0]) =>
        action.run({
          effects: targetEffects(options.eventId),
          input: options.input as { hostId: string; address: string },
          caller,
        }),
      ),
    },
  }) as unknown as Effects
}

function attachAction(ran: jest.Mock) {
  return Action.withInput(
    'attach',
    metadata,
    async ({ prefill }) =>
      InputSpec.of({
        hostId: Value.hidden<string>(),
        address: Value.select({
          name: 'Address',
          default: 'new',
          values: {
            new: 'New',
            [`${(prefill as any)?.hostId}-0`]: 'Existing',
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

describe('runAction', () => {
  test('answers the form it opened in the same procedure', async () => {
    const ran = jest.fn()
    const effects = callerEffects(attachAction(ran), 'bitcoind', 'action-event')

    await runAction({
      effects,
      packageId: 'tor',
      actionId: 'attach',
      prefill: { hostId: 'peer' },
      input: ({ spec, value }) => {
        expect(Object.keys((spec.address as any).values)).toEqual([
          'new',
          'peer-0',
        ])
        expect(value).toBeNull()
        return { hostId: 'peer', address: 'peer-0' }
      },
    })

    expect(effects.action.getInput).toHaveBeenCalledWith({
      packageId: 'tor',
      actionId: 'attach',
      eventId: 'action-event',
      prefill: { hostId: 'peer' },
    })
    expect(effects.action.run).toHaveBeenCalledWith({
      packageId: 'tor',
      actionId: 'attach',
      eventId: 'action-event',
      input: { hostId: 'peer', address: 'peer-0' },
    })
    expect(effects.eventId).toBe('action-event')
    expect(ran).toHaveBeenCalledWith(
      { hostId: 'peer', address: 'peer-0' },
      'bitcoind',
    )
  })

  test('a procedure that opened no form cannot run with input', async () => {
    const ran = jest.fn()
    const action = attachAction(ran)
    await callerEffects(action, 'bitcoind', 'init').action.getInput({
      actionId: 'attach',
    })

    await expect(
      callerEffects(action, 'bitcoind', 'main').action.run({
        actionId: 'attach',
        input: { hostId: 'peer', address: 'new' },
      }),
    ).rejects.toThrow('getActionInput has not been called')
    expect(ran).not.toHaveBeenCalled()
  })

  test('answers its form from a null-event caller', async () => {
    const ran = jest.fn()
    const effects = callerEffects(attachAction(ran), 'bitcoind', null)

    await expect(
      runAction({
        effects,
        actionId: 'attach',
        prefill: { hostId: 'peer' },
        input: () => ({ hostId: 'peer', address: 'peer-0' }),
      }),
    ).resolves.toBeNull()

    const eventId = (effects.action.getInput as jest.Mock).mock.calls[0][0]
      .eventId
    expect(eventId).toEqual(expect.any(String))
    expect(effects.action.run).toHaveBeenCalledWith({
      packageId: undefined,
      actionId: 'attach',
      eventId,
      input: { hostId: 'peer', address: 'peer-0' },
    })
    expect(effects.eventId).toBeNull()
    expect(ran).toHaveBeenCalledWith(
      { hostId: 'peer', address: 'peer-0' },
      'bitcoind',
    )
  })

  test.each(['sequential', 'concurrent'])(
    '%s calls from one null-event caller use independent forms',
    async mode => {
      const ran = jest.fn()
      const effects = callerEffects(attachAction(ran), 'bitcoind', null)
      const run = (hostId: string) =>
        runAction({
          effects,
          actionId: 'attach',
          prefill: { hostId },
          input: () => ({ hostId, address: `${hostId}-0` }),
        })

      if (mode === 'concurrent') {
        await Promise.all([run('first'), run('second')])
      } else {
        await run('first')
        await run('second')
      }

      const eventIds = (effects.action.getInput as jest.Mock).mock.calls.map(
        ([options]) => options.eventId,
      )
      expect(eventIds).toEqual([expect.any(String), expect.any(String)])
      expect(new Set(eventIds).size).toBe(2)
      expect(
        (effects.action.run as jest.Mock).mock.calls.map(
          ([options]) => options.eventId,
        ),
      ).toEqual(eventIds)
      expect(ran).toHaveBeenCalledWith(
        { hostId: 'first', address: 'first-0' },
        'bitcoind',
      )
      expect(ran).toHaveBeenCalledWith(
        { hostId: 'second', address: 'second-0' },
        'bitcoind',
      )
      expect(effects.eventId).toBeNull()
    },
  )

  test('a null-event transport needs an explicit ID to reuse a form', async () => {
    const ran = jest.fn()
    const effects = callerEffects(attachAction(ran), 'bitcoind', null)
    await effects.action.getInput({ actionId: 'attach' })

    await expect(
      effects.action.run({
        actionId: 'attach',
        input: { hostId: 'peer', address: 'new' },
      }),
    ).rejects.toThrow('getActionInput has not been called')
    expect(ran).not.toHaveBeenCalled()
  })

  test.each([null, 'caller-event'])(
    'an action with no input leaves event %s unchanged',
    async eventId => {
      const result = { version: '1' as const, message: 'Done' }
      const run = jest.fn(async () => result)
      const effects = Object.freeze({
        eventId,
        action: { getInput: jest.fn(), run },
      }) as unknown as Effects

      await expect(
        runAction({ effects, packageId: 'tor', actionId: 'reset' }),
      ).resolves.toBe(result)

      expect(effects.action.getInput).not.toHaveBeenCalled()
      expect(run).toHaveBeenCalledWith({ packageId: 'tor', actionId: 'reset' })
      expect(effects.eventId).toBe(eventId)
    },
  )

  test('propagates a no-input run failure without opening a form', async () => {
    const error = new Error('Action failed')
    const effects = {
      eventId: null,
      action: {
        getInput: jest.fn(),
        run: jest.fn(async () => {
          throw error
        }),
      },
    } as unknown as Effects

    await expect(runAction({ effects, actionId: 'reset' })).rejects.toBe(error)
    expect(effects.action.getInput).not.toHaveBeenCalled()
    expect(effects.action.run).toHaveBeenCalledWith({
      packageId: undefined,
      actionId: 'reset',
    })
  })

  test('rejects input that does not answer the opened form', async () => {
    const ran = jest.fn()
    const effects = callerEffects(attachAction(ran), 'bitcoind', null)

    await expect(
      runAction({
        effects,
        actionId: 'attach',
        prefill: { hostId: 'peer' },
        input: () => ({ hostId: 'peer', address: 'other-0' }),
      }),
    ).rejects.toThrow()
    expect(ran).not.toHaveBeenCalled()
  })

  test('does not submit when the input callback throws', async () => {
    const effects = callerEffects(attachAction(jest.fn()), 'bitcoind', null)
    const error = new Error('Cannot answer form')

    await expect(
      runAction({
        effects,
        actionId: 'attach',
        input: () => {
          throw error
        },
      }),
    ).rejects.toBe(error)
    expect(effects.action.run).not.toHaveBeenCalled()
  })

  test('does not request input or submit when no form is returned', async () => {
    const input = jest.fn(() => ({}))
    const effects = {
      eventId: null,
      action: { getInput: jest.fn(async () => null), run: jest.fn() },
    } as unknown as Effects

    await expect(
      runAction({ effects, actionId: 'attach', input }),
    ).rejects.toThrow('Action attach of this service has no input form')
    expect(input).not.toHaveBeenCalled()
    expect(effects.action.run).not.toHaveBeenCalled()
  })
})
