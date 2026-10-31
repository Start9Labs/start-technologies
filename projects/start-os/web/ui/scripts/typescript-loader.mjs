import { existsSync, readFileSync, statSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import ts from 'typescript'

export function loadTypeScript(boundaries) {
  const root = fileURLToPath(new URL('../../../../../', import.meta.url))
  const ui = resolve(root, 'projects/start-os/web/ui')
  const options = ts.readConfigFile(
    resolve(root, 'tsconfig.json'),
    ts.sys.readFile,
  ).config.compilerOptions
  const compilerOptions = ts.convertCompilerOptionsFromJson(
    options,
    root,
  ).options
  registerHooks({
    resolve(specifier, context, next) {
      if (boundaries.has(specifier))
        return { url: `test-boundary:${specifier}`, shortCircuit: true }
      let file
      if (specifier.startsWith('src/')) file = resolve(ui, specifier)
      else if (specifier.startsWith('.') && context.parentURL?.endsWith('.ts'))
        file = fileURLToPath(new URL(specifier, context.parentURL))
      if (file) {
        for (const candidate of [file, `${file}.ts`, `${file}/index.ts`]) {
          if (existsSync(candidate) && statSync(candidate).isFile())
            return { url: pathToFileURL(candidate).href, shortCircuit: true }
        }
      }
      return next(
        specifier,
        context.parentURL?.startsWith('test-boundary:')
          ? { ...context, parentURL: import.meta.url }
          : context,
      )
    },
    load(url, context, next) {
      if (url.startsWith('test-boundary:'))
        return {
          format: 'module',
          source: boundaries.get(url.slice('test-boundary:'.length)),
          shortCircuit: true,
        }
      if (url.endsWith('.ts'))
        return {
          format: 'module',
          source: ts.transpileModule(readFileSync(new URL(url), 'utf8'), {
            compilerOptions,
            fileName: fileURLToPath(url),
          }).outputText,
          shortCircuit: true,
        }
      return next(url, context)
    },
  })
}
