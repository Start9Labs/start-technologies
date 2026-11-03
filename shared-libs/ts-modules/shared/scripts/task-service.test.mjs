import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import '@angular/compiler'
import ts from 'typescript'

const serviceUrl = new URL('../src/services/task.service.ts', import.meta.url)
const boundaries = new Map([
  ['../i18n/i18n.pipe', 'export class i18nPipe {}'],
  ['./error.service', 'export class ErrorService {}'],
])
registerHooks({
  resolve(specifier, context, next) {
    const source = boundaries.get(specifier)
    if (context.parentURL !== serviceUrl.href || !source) {
      return next(specifier, context)
    }
    return {
      url: `data:text/javascript,${encodeURIComponent(source)}`,
      shortCircuit: true,
    }
  },
  load(url, context, next) {
    if (url !== serviceUrl.href) return next(url, context)
    return {
      format: 'module',
      source: ts.transpileModule(readFileSync(serviceUrl, 'utf8'), {
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
const { TaskService } = await import(serviceUrl.href)

for (const method of ['run', 'runInline']) {
  for (const reject of [false, true]) {
    test(`${method} reports ${reject ? 'failure' : 'success'} with ${method === 'run' ? 'blocking' : 'inline'} progress`, async () => {
      const events = []
      const error = new Error('Request refused')
      let resolve, fail
      const response = new Promise((yes, no) => {
        resolve = yes
        fail = no
      })
      const instance = {
        runInline: TaskService.prototype.runInline,
        i18n: { transform: value => `Localized ${value}` },
        loader: {
          open: value => {
            events.push(value)
            return {
              subscribe: () => {
                events.push('opened')
                return { unsubscribe: () => events.push('closed') }
              },
            }
          },
        },
        error: { handleError: value => events.push(value) },
      }
      const operation = TaskService.prototype[method].call(
        instance,
        async () => {
          events.push('started')
          await response
        },
        'Saving',
      )
      const expected =
        method === 'run'
          ? ['Localized Saving', 'opened', 'started']
          : ['started']
      assert.deepEqual(events, expected)
      if (reject) fail(error)
      else resolve()
      assert.equal(await operation, !reject)
      assert.deepEqual(events, [
        ...expected,
        ...(reject ? [error] : []),
        ...(method === 'run' ? ['closed'] : []),
      ])
    })
  }
}
