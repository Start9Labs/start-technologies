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

/** Routes a service's effects to one action the way StartOS does: under the calling procedure's event id. */
function callerEffects(
  action: ReturnType<typeof attachAction>,
  caller: string,
  eventId: string,
) {
  const target = { eventId } as unknown as Effects
  return {
    eventId,
    action: {
      getInput: jest.fn(async ({ prefill }: { prefill?: unknown }) =>
        action.getInput({
          effects: target,
          prefill: (prefill ?? null) as any,
          caller,
        }),
      ),
      run: jest.fn(async ({ input }: { input?: any }) =>
        action.run({ effects: target, input, caller }),
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
    const effects = callerEffects(attachAction(ran), 'bitcoind', 'init')

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
