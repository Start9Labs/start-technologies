import { Effects } from '../Effects'
import { List } from '../actions/input/builder/list'
import { Value } from '../actions/input/builder/value'

const options = { effects: {} as Effects, prefill: null }

const fields = {
  text: (required: boolean) =>
    Value.text({ name: 'T', required, default: null }),
  dynamicText: (required: boolean) =>
    Value.dynamicText(async () => ({ name: 'T', required, default: null })),
  textarea: (required: boolean) =>
    Value.textarea({ name: 'T', required, default: null }),
  dynamicTextarea: (required: boolean) =>
    Value.dynamicTextarea(async () => ({
      name: 'T',
      required,
      default: null,
    })),
  color: (required: boolean) =>
    Value.color({ name: 'T', required, default: null }),
  dynamicColor: (required: boolean) =>
    Value.dynamicColor(async () => ({ name: 'T', required, default: null })),
  datetime: (required: boolean) =>
    Value.datetime({ name: 'T', required, default: null }),
  dynamicDatetime: (required: boolean) =>
    Value.dynamicDatetime(async () => ({
      name: 'T',
      required,
      default: null,
    })),
}

describe.each(Object.entries(fields))('%s', (_, field) => {
  test.each([true, false])(
    'refuses an empty value when required is %s',
    async required => {
      const value = field(required)
      const { validator } = await value.build(options as any)
      expect(() => validator.parse('')).toThrow()
      expect(() => value.validator.parse('')).toThrow()
      expect(validator.parse('x')).toBe('x')
    },
  )

  test('an optional field accepts null', async () => {
    const { validator } = await field(false).build(options as any)
    expect(validator.parse(null)).toBeNull()
  })
})

describe.each([
  ['list text', () => List.text({ name: 'L' }, {})],
  [
    'list dynamicText',
    () => List.dynamicText(async () => ({ name: 'L', spec: {} })),
  ],
])('%s', (_, list) => {
  test('refuses an empty item', async () => {
    const value = list()
    const { validator } = await value.build(options as any)
    expect(() => validator.parse([''])).toThrow()
    expect(() => value.validator.parse([''])).toThrow()
    expect(validator.parse(['x'])).toEqual(['x'])
  })
})
