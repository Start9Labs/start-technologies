import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import {
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'check-i18n-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  const modules = join(root, 'shared-libs/ts-modules')
  const dictionaries = join(modules, 'shared/src/i18n/dictionaries')
  const script = join(modules, 'scripts/check-i18n.mjs')
  mkdirSync(dictionaries, { recursive: true })
  mkdirSync(join(modules, 'scripts'))
  for (const directory of [
    'marketplace',
    '../../projects/start-os/web/ui',
    '../../projects/start-os/web/setup-wizard',
    '../../projects/brochure-marketplace',
  ])
    mkdirSync(join(modules, directory), { recursive: true })
  copyFileSync(new URL('./check-i18n.mjs', import.meta.url), script)
  writeFileSync(
    join(dictionaries, 'en.ts'),
    `export const ENGLISH = {
  'Simple key': 1,
  "Server's certificate": 2,
}\n`,
  )
  for (const language of ['de', 'es', 'fr', 'pl']) {
    writeFileSync(
      join(dictionaries, `${language}.ts`),
      `export default {
  1: 'Simple translation',
  2: 'Certificate translation',
}\n`,
    )
  }
  return {
    dictionaries,
    template: join(modules, 'shared/probe.html'),
    run: () => spawnSync(process.execPath, [script], { encoding: 'utf8' }),
  }
}

test('accepts both English dictionary quote styles, including an apostrophe', t => {
  const f = fixture(t)
  writeFileSync(
    f.template,
    `{{ 'Simple key' | i18n }} {{ "Server's certificate" | i18n }}`,
  )
  const result = f.run()
  assert.equal(result.status, 0, result.stderr)
  assert.match(result.stdout, /Loaded 2 i18n keys/)
  assert.match(result.stdout, /All i18n keys are valid/)
})

test('requires translations for double-quoted English entries even when unused', t => {
  const f = fixture(t)
  writeFileSync(
    join(f.dictionaries, 'de.ts'),
    `export default {
  1: 'Simple translation',
}\n`,
  )
  const result = f.run()
  assert.equal(result.status, 1)
  assert.match(result.stderr, /de\.ts is missing keys: 2/)
})
