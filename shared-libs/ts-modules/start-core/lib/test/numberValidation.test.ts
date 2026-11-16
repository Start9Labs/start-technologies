import { Value } from '../actions/input/builder/value'
import { Effects } from '../Effects'

const options = {
  name: 'Number',
  default: null,
  required: true,
  integer: true,
  min: -2,
  max: 2,
} as const

for (const [name, field] of [
  ['static', Value.number(options)],
  ['dynamic', Value.dynamicNumber(async () => options)],
] as const) {
  test.each([
    [-3, false],
    [-2, true],
    [-0.5, false],
    [0, true],
    [2, true],
    [3, false],
  ])(
    `${name} numeric field validates %s before the action runs`,
    async (input, valid) => {
      const built = await field.build({ effects: {} as Effects, prefill: null })
      expect(built.validator.safeParse(input).success).toBe(valid)
    },
  )
}

test.each([
  [{ min: 0 }, -1, false],
  [{ min: 0 }, 0, true],
  [{ max: 0 }, 1, false],
  [{ max: 0 }, 0, true],
])('zero numeric bounds are enforced: %j', async (bounds, input, valid) => {
  const field = Value.number({ ...options, min: null, max: null, ...bounds })
  const built = await field.build({ effects: {} as Effects, prefill: null })
  expect(built.validator.safeParse(input).success).toBe(valid)
  expect(field.validator.safeParse(input).success).toBe(valid)
})

test.each([null, undefined])(
  'nullable number preserves missing value %j',
  async input => {
    const field = Value.number({ ...options, required: false })
    const built = await field.build({ effects: {} as Effects, prefill: null })
    expect(built.validator.parse(input)).toBeNull()
  },
)
