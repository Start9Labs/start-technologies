import { z } from '../zExport'

const nested = { host: z.string(), opts: z.looseObject({ tls: z.boolean() }) }
const extra = { host: 'h', opts: { tls: true, retries: 3 }, comment: 'keep' }

describe('the z re-export', () => {
  test('z.object strips unknown keys', () => {
    expect(z.object({ host: z.string() }).parse(extra)).toEqual({ host: 'h' })
  })

  test('z.looseObject preserves unknown keys at every level', () => {
    expect(z.looseObject(nested).parse(extra)).toEqual(extra)
  })

  test('z.deepLoose makes a strip schema preserve unknown keys', () => {
    const strict = z.object({
      host: z.string(),
      opts: z.object({ tls: z.boolean() }),
    })
    expect(z.deepLoose(strict).parse(extra)).toEqual(extra)
  })

  test('z.deepLoose reaches through optional, array and record wrappers', () => {
    const schema = z.object({
      maybe: z.object({ a: z.string() }).optional(),
      list: z.array(z.object({ b: z.string() })),
      map: z.record(z.string(), z.object({ c: z.string() })),
    })
    expect(
      z.deepLoose(schema).parse({
        maybe: { a: 'a', x: 1 },
        list: [{ b: 'b', y: 2 }],
        map: { k: { c: 'c', z: 3 } },
      }),
    ).toEqual({
      maybe: { a: 'a', x: 1 },
      list: [{ b: 'b', y: 2 }],
      map: { k: { c: 'c', z: 3 } },
    })
  })

  test.each(['type', ''])(
    'z.deepLoose keeps a discriminated union keyed on %j',
    discriminator => {
      const schema = z.discriminatedUnion(discriminator, [
        z.object({
          [discriminator]: z.literal('a').catch('a'),
          x: z.string().catch(''),
        }),
        z.object({ [discriminator]: z.literal('b').catch('b') }),
      ])
      expect(z.deepLoose(schema).parse({ [discriminator]: 'b', y: 1 })).toEqual(
        {
          [discriminator]: 'b',
          y: 1,
        },
      )
    },
  )

  test('z.deepPartial makes every level optional and loose', () => {
    const schema = z.object({
      host: z.string(),
      opts: z.object({ tls: z.boolean() }),
    })
    expect(z.deepPartial(schema).parse({ opts: { retries: 3 } })).toEqual({
      opts: { retries: 3 },
    })
  })
})
