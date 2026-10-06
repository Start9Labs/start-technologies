import { InputSpec } from '../actions/input/builder/inputSpec'
import { Value } from '../actions/input/builder/value'
import { Variants } from '../actions/input/builder/variants'
import { z } from '../zExport'

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
})

describe('InputSpec.partialValidator', () => {
  test('accepts any subset of the fields', () => {
    expect(spec.partialValidator.parse({ rpc: {} })).toEqual({ rpc: {} })
  })

  test('keeps keys the spec does not declare, at every level', () => {
    const data = {
      rpc: { port: 1, rpcauth: 'kept' },
      mode: { selection: 'a', value: { x: true, y: 'kept' } },
      shrinkdebugfile: 1,
    }
    expect(spec.partialValidator.parse(data)).toEqual(data)
  })

  test('still rejects a declared key of the wrong type', () => {
    expect(() => spec.partialValidator.parse({ rpc: { port: 'x' } })).toThrow()
  })

  test('keeps a hidden field’s own shape: z.object strips, a union stays keyed', () => {
    const hidden = InputSpec.of({
      raw: Value.hidden(
        z.object({
          backend: z.discriminatedUnion('type', [
            z.looseObject({ type: z.literal('a'), x: z.string().catch('') }),
            z.looseObject({ type: z.literal('b'), y: z.string() }),
          ]),
        }),
      ),
    })
    expect(
      hidden.partialValidator.parse({
        raw: { backend: { type: 'b', y: 'kept' }, dropped: 1 },
      }),
    ).toEqual({ raw: { backend: { type: 'b', y: 'kept' } } })
    expect(() =>
      hidden.partialValidator.parse({ raw: { backend: { y: 'kept' } } }),
    ).toThrow()
  })

  test('accepts a subset of enum record keys and validates supplied entries', () => {
    const record = InputSpec.of({
      raw: Value.hidden(z.record(z.enum(['a', 'b']), z.string())),
    })
    expect(record.partialValidator.parse({ raw: { a: 'kept' } })).toEqual({
      raw: { a: 'kept' },
    })
    expect(record.partialValidator.parse({ raw: {} })).toEqual({ raw: {} })
    expect(() => record.partialValidator.parse({ raw: { a: 1 } })).toThrow()
    expect(() =>
      record.partialValidator.parse({ raw: { c: 'unknown' } }),
    ).toThrow()
  })

  test('reaches inside a .catch() and keeps its fallback', () => {
    const caught = InputSpec.of({
      raw: Value.hidden(
        z.looseObject({ a: z.string(), b: z.number() }).catch({ a: 'x', b: 0 }),
      ),
    })
    expect(caught.partialValidator.parse({ raw: { a: 'y' } })).toEqual({
      raw: { a: 'y' },
    })
    expect(caught.partialValidator.parse({ raw: { a: 1 } })).toEqual({
      raw: { a: 'x', b: 0 },
    })
  })
})
