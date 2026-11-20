import { Effects } from '../Effects'
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
  test('a required field refuses an empty value from its action input', async () => {
    const { validator } = await field(true).build(options as any)
    expect(() => validator.parse('')).toThrow()
    expect(validator.parse('x')).toBe('x')
  })

  test('an optional field still accepts an empty value', async () => {
    const { validator } = await field(false).build(options as any)
    expect(validator.parse('')).toBe('')
  })
})

test('the static validator a file model reads with keeps an empty value', () => {
  expect(
    Value.text({ name: 'T', required: true, default: null }).validator.parse(
      '',
    ),
  ).toBe('')
})
