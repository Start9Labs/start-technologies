import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'
import '@angular/compiler'
import ts from 'typescript'

const configPath = fileURLToPath(
  new URL('../../../../tsconfig.json', import.meta.url),
)
const compilerOptions = ts.convertCompilerOptionsFromJson(
  ts.readConfigFile(configPath, ts.sys.readFile).config.compilerOptions,
  dirname(configPath),
).options

registerHooks({
  load(url, context, next) {
    if (!url.endsWith('.ts')) return next(url, context)
    return {
      format: 'module',
      source: ts.transpileModule(readFileSync(new URL(url), 'utf8'), {
        compilerOptions,
      }).outputText,
      shortCircuit: true,
    }
  },
})
const { filterPackages, isNewArrival, NEW_ARRIVALS } =
  await import('../src/pipes/filter-packages.pipe.ts')

const DAY = 24 * 60 * 60 * 1000
const pkg = (id, firstPublishedAt) => ({
  id,
  title: id,
  categories: [],
  firstPublishedAt,
  s9pks: [],
})
const fresh = pkg('fresh', new Date(Date.now() - 29 * DAY).toISOString())
const stale = pkg('stale', new Date(Date.now() - 31 * DAY).toISOString())
const unknown = pkg('unknown', undefined)

test('a package first published within 30 days is a new arrival', () => {
  assert.equal(isNewArrival(fresh), true)
  assert.equal(isNewArrival(stale), false)
  assert.equal(isNewArrival(unknown), false)
})

test('the new arrivals category lists new arrivals alone', () => {
  assert.deepEqual(
    filterPackages([fresh, stale, unknown], '', NEW_ARRIVALS, 'a').map(
      p => p.id,
    ),
    ['fresh'],
  )
})
