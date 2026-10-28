import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import '@angular/compiler'
import { signal } from '@angular/core'
import ts from 'typescript'

const directiveUrl = new URL(
  '../src/directives/icon.directive.ts',
  import.meta.url,
)
registerHooks({
  load(url, context, next) {
    if (url !== directiveUrl.href) return next(url, context)
    return {
      format: 'module',
      source: ts.transpileModule(readFileSync(directiveUrl, 'utf8'), {
        compilerOptions: {
          target: ts.ScriptTarget.ES2022,
          module: ts.ModuleKind.ES2022,
          experimentalDecorators: true,
        },
      }).outputText,
      shortCircuit: true,
    }
  },
})
const { IconDirective } = await import(directiveUrl.href)

class Icon extends IconDirective {
  src = signal(null)
}

test('a resolved URL stays loading until the image settles', () => {
  const icon = new Icon()
  assert.equal(icon.loading(), true)
  icon.src.set('https://registry.example/icon.png')
  assert.equal(icon.loading(), true)
  icon.settled.set(true)
  assert.equal(icon.loading(), false)
})

test('changing source resets settled state, including a previously loaded URL', () => {
  const icon = new Icon()
  icon.src.set('first.png')
  icon.settled.set(true)
  assert.equal(icon.loading(), false)
  icon.src.set('second.png')
  assert.equal(icon.loading(), true)
  icon.settled.set(true)
  assert.equal(icon.loading(), false)
  icon.src.set('first.png')
  assert.equal(icon.loading(), true)
})

test('clearing the source returns to loading', () => {
  const icon = new Icon()
  icon.src.set('icon.png')
  icon.settled.set(true)
  assert.equal(icon.loading(), false)
  icon.src.set(null)
  assert.equal(icon.loading(), true)
})
