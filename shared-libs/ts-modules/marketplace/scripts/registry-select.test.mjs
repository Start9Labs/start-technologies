import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
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

const shared = new URL('../../shared/src/', import.meta.url).href
const boundaries = new Map([
  [
    '@start9labs/shared',
    `export * from '${shared}util/misc.util.ts'
export { defaultRegistries, isDefaultRegistry } from '${shared}types/workspace-config.ts'
export class DialogService {}
export class TaskService {}
export class i18nPipe {}
export const FALLBACK_ICON = "fallback.png"`,
  ],
  [
    '@taiga-ui/core',
    `import { InjectionToken } from '@angular/core'
export const TUI_BREAKPOINT = new InjectionToken('TUI_BREAKPOINT')
export class TuiButton {}
export class TuiDataList {}
export class TuiDropdown {}
export class TuiIcon {}
export class TuiTitle {}`,
  ],
  ['@taiga-ui/kit', 'export class TuiAvatar {}\nexport class TuiFade {}'],
  ['@start9labs/start-core', 'export const RPC = {}\nexport const T = {}'],
  ['./store-icon.directive', 'export class StoreIconDirective {}'],
])

registerHooks({
  resolve(specifier, context, next) {
    if (boundaries.has(specifier))
      return { url: `test-boundary:${specifier}`, shortCircuit: true }
    if (specifier.startsWith('.') && context.parentURL?.endsWith('.ts')) {
      const url = new URL(specifier, context.parentURL)
      for (const candidate of [url.href, `${url.href}.ts`]) {
        if (existsSync(new URL(candidate)) && candidate.endsWith('.ts'))
          return { url: candidate, shortCircuit: true }
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

const {
  createEnvironmentInjector,
  runInInjectionContext,
  ɵSIGNAL: SIGNAL,
} = await import('@angular/core')
const { Router } = await import('@angular/router')
const { BehaviorSubject } = await import('rxjs')
const { DialogService, TaskService } = await import('@start9labs/shared')
const { TUI_BREAKPOINT } = await import('@taiga-ui/core')
const { AbstractMarketplaceService } =
  await import('../src/services/abstract-marketplace.service.ts')
const { MarketplaceRegistrySelectComponent } =
  await import('../src/components/registry-select.component.ts')

const start9 = 'https://registry.start9.com/'
const community = 'https://community-registry.start9.com/'

function picker({ registries, currentUrl, prompt = '' }) {
  const failures = []
  const connected = []
  const service = {
    registries$: new BehaviorSubject(registries),
    currentRegistryUrl$: new BehaviorSubject(currentUrl),
    currentRegistry$: new BehaviorSubject(undefined),
    connect: async url => connected.push(url),
  }
  const injector = createEnvironmentInjector(
    [
      { provide: AbstractMarketplaceService, useValue: service },
      { provide: Router, useValue: { navigate: () => {} } },
      { provide: TUI_BREAKPOINT, useValue: () => 'desktopLarge' },
      {
        provide: DialogService,
        useValue: {
          openPrompt: () => new BehaviorSubject(prompt),
        },
      },
      {
        provide: TaskService,
        useValue: {
          run: async fn => {
            try {
              await fn()
            } catch (e) {
              failures.push(e)
            }
          },
        },
      },
    ],
    null,
  )
  const component = runInInjectionContext(
    injector,
    () => new MarketplaceRegistrySelectComponent(),
  )
  return { component, connected, failures }
}

test('an invalid custom URL fails inside the task instead of the event handler', async () => {
  const { component, connected, failures } = picker({
    registries: [],
    currentUrl: start9,
    prompt: 'https://',
  })

  await component.viewCustom()
  await new Promise(setImmediate)

  assert.equal(failures.length, 1)
  assert.deepEqual(connected, [])
})

test('saved custom registries survive the link input binding after the streams replay', () => {
  const saved = 'https://saved.example/'
  const { component } = picker({
    registries: [
      { url: start9, name: 'Start9 Registry' },
      { url: community, name: 'Community Registry' },
      { url: saved, name: 'Saved' },
    ],
    currentUrl: 'https://unsaved.example/',
  })
  const link = component.manageRegistriesLink[SIGNAL]
  link.applyValueToInputSignal(link, '/system/registries')

  assert.deepEqual(
    component['data']().custom.map(r => r.url),
    [saved, 'https://unsaved.example/'],
  )
})
