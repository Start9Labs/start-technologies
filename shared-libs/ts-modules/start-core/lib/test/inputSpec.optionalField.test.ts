import { InputSpec } from '../actions/input/builder/inputSpec'
import { Value } from '../actions/input/builder/value'

const spec = InputSpec.of({
  keyHash: Value.text({ name: 'Key hash', required: true, default: null }),
  label: Value.text({ name: 'Label', required: false, default: null }),
  limit: Value.number({
    name: 'Limit',
    required: false,
    default: null,
    integer: true,
  }),
})

describe('a field that is not required', () => {
  test('reads an omitted key as null', () => {
    expect(spec.validator.parse({ keyHash: 'h' })).toEqual({
      keyHash: 'h',
      label: null,
      limit: null,
    })
  })

  test('keeps a submitted value', () => {
    expect(
      spec.validator.parse({ keyHash: 'h', label: 'l', limit: 1 }),
    ).toEqual({ keyHash: 'h', label: 'l', limit: 1 })
  })

  test('still rejects a value of the wrong type', () => {
    expect(() => spec.validator.parse({ keyHash: 'h', label: 1 })).toThrow()
  })

  test('leaves a required field required', () => {
    expect(() => spec.validator.parse({ label: 'l' })).toThrow()
  })

  test('stays absent from a partial parse', () => {
    expect(spec.partialValidator.parse({})).toEqual({})
  })
})
