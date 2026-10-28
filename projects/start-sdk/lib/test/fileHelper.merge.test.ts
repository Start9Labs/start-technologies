import { mkdtempSync, readFileSync, writeFileSync } from 'fs'
import { tmpdir } from 'os'
import { join } from 'path'
import { FileHelper } from '../util/fileHelper'
import * as z from '@start9labs/start-core/z'

const shape = z.looseObject({ A: z.string(), K: z.string().optional() })
const effects = {} as any

const dir = mkdtempSync(join(tmpdir(), 'file-helper-'))

const seeded = (name: string, contents: string) => {
  const path = join(dir, name)
  writeFileSync(path, contents)
  return path
}

describe('FileHelper.merge', () => {
  // Every format drops a key merged as `undefined`; the env writer used to
  // render it through a template literal and emit the word instead.
  test.each([
    ['env', 'A=keep\nK=old\n', 'A=keep'],
    ['ini', 'A=keep\nK=old\n', 'A=keep'],
    ['json', '{"A":"keep","K":"old"}', '{\n  "A": "keep"\n}'],
    ['yaml', 'A: keep\nK: old\n', 'A: keep'],
    ['toml', 'A = "keep"\nK = "old"\n', 'A = "keep"'],
  ])('%s removes a key merged as undefined', async (kind, before, after) => {
    const path = seeded(`undefined.${kind}`, before)
    const file = (FileHelper as any)[kind](path, shape)

    await file.merge(effects, { K: undefined })

    expect(readFileSync(path, 'utf-8').trim()).toBe(after)
  })

  test.each(['env', 'ini', 'json', 'yaml', 'toml'])(
    '%s leaves a key the merge does not name',
    async kind => {
      const before = {
        env: 'A=keep\nK=old\n',
        ini: 'A=keep\nK=old\n',
        json: '{"A":"keep","K":"old"}',
        yaml: 'A: keep\nK: old\n',
        toml: 'A = "keep"\nK = "old"\n',
      }[kind]!
      const path = seeded(`absent.${kind}`, before)
      const file = (FileHelper as any)[kind](path, shape)

      await file.merge(effects, {})

      expect(readFileSync(path, 'utf-8')).toContain('old')
    },
  )

  // A transformer can leave a key undefined that the data never named, the
  // way a form-to-file mapping does for an unset setting.
  test.each(['env', 'ini', 'yaml', 'toml'])(
    '%s drops a key its onWrite leaves undefined',
    async kind => {
      const path = seeded(`transformed.${kind}`, '')
      const file = (FileHelper as any)[kind](
        path,
        shape,
        ...(kind === 'ini' || kind === 'yaml' ? [undefined] : []),
        {
          onRead: (raw: Record<string, unknown>) => raw,
          onWrite: (data: { A: string }) => ({ A: data.A, K: undefined }),
        },
      )

      await file.write(effects, { A: 'keep' })

      expect(readFileSync(path, 'utf-8')).not.toContain('undefined')
      expect(readFileSync(path, 'utf-8')).toContain('keep')
    },
  )

  test('raw hands its writer no key merged as undefined', async () => {
    const path = seeded('undefined.raw', 'A=keep\nK=old\n')
    const file = FileHelper.raw(
      path,
      (data: { A: string; K?: string }) =>
        Object.entries(data)
          .map(([k, v]) => `${k}=${v}`)
          .join('\n'),
      raw => Object.fromEntries(raw.split('\n').map(line => line.split('='))),
      data => shape.parse(data),
    )

    await file.merge(effects, { K: undefined })

    expect(readFileSync(path, 'utf-8')).toBe('A=keep')
  })

  test('drops an undeclared key under z.object, keeps it under z.looseObject', async () => {
    const path = seeded('unknown.json', '{"A":"keep","X":"extra"}')

    await FileHelper.json(path, z.object({ A: z.string() })).merge(effects, {})
    expect(JSON.parse(readFileSync(path, 'utf-8'))).toEqual({ A: 'keep' })

    writeFileSync(path, '{"A":"keep","X":"extra"}')
    await FileHelper.json(path, shape).merge(effects, {})
    expect(JSON.parse(readFileSync(path, 'utf-8'))).toEqual({
      A: 'keep',
      X: 'extra',
    })
  })

  test('merges a discriminated union as the variant it names', async () => {
    const path = seeded('union.json', '{}')
    const file = FileHelper.json(
      path,
      z.looseObject({
        backend: z
          .discriminatedUnion('type', [
            z.looseObject({
              type: z.literal('a').catch('a'),
              x: z.string().catch(''),
            }),
            z.looseObject({ type: z.literal('b').catch('b') }),
          ])
          .optional(),
      }),
    )

    await file.merge(effects, { backend: { type: 'b' } })

    expect(JSON.parse(readFileSync(path, 'utf-8'))).toEqual({
      backend: { type: 'b' },
    })
  })

  test('changes only the key it names when keys share a .catch() object', async () => {
    const library = z
      .looseObject({ enabled: z.boolean() })
      .catch({ enabled: false })
    const path = seeded('shared-catch.json', '{}')
    const file = FileHelper.json(
      path,
      z.looseObject({ a: library, b: library }),
    )

    await file.merge(effects, { a: { enabled: true } })

    expect(JSON.parse(readFileSync(path, 'utf-8'))).toEqual({
      a: { enabled: true },
      b: { enabled: false },
    })
  })

  test('leaves a .catch() object as it was', async () => {
    const fallback = { journal_mode: 'wal' }
    const shape = z.looseObject({
      db: z.looseObject({ journal_mode: z.string() }).catch(fallback),
    })
    const path = seeded('catch-object.json', '{}')

    await FileHelper.json(path, shape).merge(effects, {
      db: { journal_mode: 'delete' },
    })

    expect(JSON.parse(readFileSync(path, 'utf-8'))).toEqual({
      db: { journal_mode: 'delete' },
    })
    expect(fallback).toEqual({ journal_mode: 'wal' })
    expect(shape.parse({})).toEqual({ db: { journal_mode: 'wal' } })
  })

  test('merges nested objects key by key and replaces arrays', async () => {
    const path = seeded(
      'deep.json',
      JSON.stringify({
        server: { host: 'a', port: 1, extra: { keep: true } },
        tags: ['x', 'y'],
      }),
    )
    const file = FileHelper.json(
      path,
      z.looseObject({
        server: z.looseObject({ host: z.string(), port: z.number() }),
        tags: z.array(z.string()),
      }),
    )

    await file.merge(effects, { server: { port: 2 }, tags: ['z'] })

    expect(JSON.parse(readFileSync(path, 'utf-8'))).toEqual({
      server: { host: 'a', port: 2, extra: { keep: true } },
      tags: ['z'],
    })
  })

  test('replaces a date with the one merged', async () => {
    const path = seeded('date.toml', 'when = 2020-01-01T00:00:00Z\n')
    const file = FileHelper.toml(path, z.looseObject({ when: z.date() }))

    await file.merge(effects, { when: new Date('2026-10-27T00:00:00Z') })

    expect(readFileSync(path, 'utf-8')).toContain('2026-10-27')
  })

  test('yaml keeps a custom tag that parses to a class instance', async () => {
    class Include {
      constructor(public value: string) {}
    }
    const include = {
      tag: '!include',
      identify: (v: unknown) => v instanceof Include,
      resolve: (v: string) => new Include(v),
      stringify: ({ value }: { value: unknown }) => (value as Include).value,
    }
    const path = seeded(
      'tags.yaml',
      'automation: !include automations.yaml\nhttp:\n  port: 1\n',
    )
    const file = FileHelper.yaml(path, z.looseObject({ http: z.any() }), {
      customTags: [include],
    })

    await file.merge(effects, { http: undefined })

    expect(readFileSync(path, 'utf-8').trim()).toBe(
      'automation: !include automations.yaml',
    )
  })
})
