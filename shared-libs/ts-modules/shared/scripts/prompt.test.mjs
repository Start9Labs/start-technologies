import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import '@angular/compiler'
import { Subject } from 'rxjs'
import ts from 'typescript'

const componentUrl = new URL(
  '../src/components/prompt.component.ts',
  import.meta.url,
)
registerHooks({
  resolve(specifier, context, next) {
    if (
      context.parentURL !== componentUrl.href ||
      specifier !== '../i18n/i18n.pipe'
    )
      return next(specifier, context)
    return {
      url: 'data:text/javascript,export class i18nPipe {}',
      shortCircuit: true,
    }
  },
  load(url, context, next) {
    if (url !== componentUrl.href) return next(url, context)
    return {
      format: 'module',
      source: ts.transpileModule(readFileSync(componentUrl, 'utf8'), {
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
const { PromptModal } = await import(componentUrl.href)

for (const [useMask, raw, expected] of [
  [true, '  secret  ', '  secret  '],
  [true, '   ', '   '],
  [false, '  name  ', 'name'],
]) {
  test(`prompt preserves masked=${useMask} input ${JSON.stringify(raw)}`, () => {
    const values = []
    const result = new Subject()
    result.subscribe(value => values.push(value))
    const prompt = Object.create(PromptModal.prototype)
    prompt.context = { data: { useMask, required: true }, $implicit: result }
    prompt.submit(raw)
    assert.deepEqual(values, [expected])
    result.complete()
  })
}
