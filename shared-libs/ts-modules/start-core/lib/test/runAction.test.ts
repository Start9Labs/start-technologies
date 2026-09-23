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

/** Routes the effects to one action the way StartOS does: a fresh event id per call unless one is named. */
function hostedBy(action: ReturnType<typeof attachAction>, caller: string) {
  let next = 0
  const effectsFor = (eventId: string) => ({ eventId }) as unknown as Effects
  return {
    eventId: 'caller-event',
    action: {
      getInput: jest.fn(async ({ prefill }: { prefill?: unknown }) =>
        action.getInput({
          effects: effectsFor(`event-${next++}`),
          prefill: (prefill ?? null) as any,
          caller,
        }),
      ),
      run: jest.fn(
        async ({ eventId, input }: { eventId?: string; input?: any }) =>
          action.run({
            effects: effectsFor(eventId ?? `event-${next++}`),
            input,
            caller,
          }),
      ),
    },
  } as unknown as Effects
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
          values: { new: 'New', [`${(prefill as any)?.hostId}-0`]: 'Existing' },
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
  test('answers the form it opened, under that form’s event id', async () => {
    const ran = jest.fn()
    const effects = hostedBy(attachAction(ran), 'bitcoind')

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
      prefill: { hostId: 'peer' },
    })
    expect(effects.action.run).toHaveBeenCalledWith(
      expect.objectContaining({
        packageId: 'tor',
        actionId: 'attach',
        eventId: 'event-0',
      }),
    )
    expect(ran).toHaveBeenCalledWith(
      { hostId: 'peer', address: 'peer-0' },
      'bitcoind',
    )
  })

  test('an input run under an event id no form was opened for is refused', async () => {
    const ran = jest.fn()
    const effects = hostedBy(attachAction(ran), 'bitcoind')

    await expect(
      effects.action.run({
        actionId: 'attach',
        input: { hostId: 'peer', address: 'new' },
      }),
    ).rejects.toThrow('getActionInput has not been called')
    expect(ran).not.toHaveBeenCalled()
  })

  test('an action with no input runs without opening a form', async () => {
    const run = jest.fn(async () => null)
    const effects = {
      action: { getInput: jest.fn(), run },
    } as unknown as Effects

    await runAction({ effects, packageId: 'tor', actionId: 'reset' })

    expect(effects.action.getInput).not.toHaveBeenCalled()
    expect(run).toHaveBeenCalledWith({ packageId: 'tor', actionId: 'reset' })
  })
})
