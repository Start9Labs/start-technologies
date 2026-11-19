import { Effects } from '../Effects'
import { InputSpec } from '../actions/input/builder/inputSpec'
import { List } from '../actions/input/builder/list'
import { Value } from '../actions/input/builder/value'
import { Variants } from '../actions/input/builder/variants'

const spec = InputSpec.of({
  rpc: Value.object(
    { name: 'RPC' },
    InputSpec.of({
      port: Value.number({
        name: 'Port',
        required: true,
        default: 8332,
        integer: true,
      }),
    }),
  ),
  mode: Value.union({
    name: 'Mode',
    default: 'a',
    variants: Variants.of({
      a: {
        name: 'A',
        spec: InputSpec.of({ x: Value.toggle({ name: 'X', default: false }) }),
      },
      b: { name: 'B', spec: InputSpec.of({}) },
    }),
  }),
  peers: Value.list(
    List.obj(
      { name: 'Peers' },
      {
        spec: InputSpec.of({
          host: Value.text({ name: 'Host', required: true, default: null }),
        }),
      },
    ),
  ),
})

const submitted = {
  rpc: { port: 1, rpcauth: 'injected' },
  mode: { selection: 'a', value: { x: true, y: 'injected' }, z: 'injected' },
  peers: [{ host: 'h', extra: 'injected' }],
  raw: { startupnotify: 'injected' },
}

describe('the validator an action runs its input through', () => {
  test('drops keys the form does not declare, at every level', async () => {
    const { validator } = await spec.build({
      effects: {} as Effects,
      prefill: null,
    })
    expect(validator.parse(submitted)).toEqual({
      rpc: { port: 1 },
      mode: { selection: 'a', value: { x: true } },
      peers: [{ host: 'h' }],
    })
  })

  test('drops union history, which only the form keeps', async () => {
    const { validator } = await Variants.of({
      selected: { name: 'Selected', spec: InputSpec.of({}) },
      previous: { name: 'Previous', spec },
    }).build({ effects: {} as Effects, prefill: null })
    expect(
      await validator.parseAsync({
        selection: 'selected',
        value: {},
        other: { previous: submitted },
      }),
    ).toEqual({ selection: 'selected', value: {} })
  })

  test('leaves the static validators keeping them, for file models', () => {
    expect(spec.validator.parse(submitted)).toEqual(submitted)
    expect(spec.partialValidator.parse(submitted)).toEqual(submitted)
  })
})
