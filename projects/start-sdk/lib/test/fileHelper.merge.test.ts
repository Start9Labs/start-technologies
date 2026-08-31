import { mkdtempSync, readFileSync, writeFileSync } from 'fs'
import { tmpdir } from 'os'
import { join } from 'path'
import { FileHelper } from '../util/fileHelper'
import { z } from '@start9labs/start-core/zExport'

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
})
