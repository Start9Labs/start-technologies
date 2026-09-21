import { Action } from '../actions/setupActions'
import { InputSpec } from '../actions/input/builder/inputSpec'
import { Value } from '../actions/input/builder/value'
import { Effects } from '../Effects'

describe('action caller', () => {
  const effects = { eventId: 'event' } as unknown as Effects
  const metadata = {
    name: 'Register',
    description: '',
    warning: null,
    allowedStatuses: 'any' as const,
    group: null,
    visibility: 'hidden' as const,
  }

  test.each([
    ['a service', 'lnd', 'lnd'],
    ['the user', null, null],
    ['a runtime that names nobody', undefined, null],
  ])('run sees %s', async (_name, caller, expected) => {
    const run = jest.fn(async () => null)
    const action = Action.withoutInput('register', metadata, run)

    await action.run({ effects, input: {}, caller })

    expect(run).toHaveBeenCalledWith(
      expect.objectContaining({ caller: expected }),
    )
  })

  test('the input spec, the prefill and run all see the same caller', async () => {
    const seen: Record<string, unknown> = {}
    const action = Action.withInput(
      'register',
      metadata,
      async ({ caller }) => {
        seen.spec = caller
        return InputSpec.of({ hostId: Value.hidden<string>() })
      },
      async ({ caller }) => {
        seen.prefill = caller
        return null
      },
      async ({ caller }) => {
        seen.run = caller
        return null
      },
    )

    await action.getInput({ effects, prefill: null, caller: 'lnd' })
    await action.run({ effects, input: { hostId: 'peer' }, caller: 'lnd' })

    expect(seen).toEqual({ spec: 'lnd', prefill: 'lnd', run: 'lnd' })
  })
})
