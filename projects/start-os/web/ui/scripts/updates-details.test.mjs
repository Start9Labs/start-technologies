import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import {
  createEnvironmentInjector,
  runInInjectionContext,
  signal,
  ɵChangeDetectionScheduler,
  ɵEffectScheduler,
  ɵINJECTOR_SCOPE,
} from '@angular/core'
import { SIGNAL } from '@angular/core/primitives/signals'
import { Subject } from 'rxjs'
import { loadTypeScript } from './typescript-loader.mjs'

const boundaries = new Map()
const classes = (path, names, extra = '') =>
  boundaries.set(
    path,
    names.map(name => `export class ${name} {}`).join('\n') + extra,
  )
classes(
  '@start9labs/shared',
  [
    'DialogService',
    'Exver',
    'i18nPipe',
    'LocalizePipe',
    'MarkdownPipe',
    'SafeLinksDirective',
  ],
  'export const getErrorMessage = error => error.message;',
)
classes(
  '@start9labs/marketplace',
  ['MarketplaceIconDirective', 'MarketplaceSignerChipsComponent'],
  `export { pendingReleases } from ${JSON.stringify(new URL('../../../../../shared-libs/ts-modules/marketplace/src/util/releases.ts', import.meta.url).href)};
export const packageSigners = () => []; export const matchingTrustedKeys = () => [];`,
)
classes('@taiga-ui/core', ['TuiButton', 'TuiExpand', 'TuiIcon', 'TuiTitle'])
classes('@taiga-ui/dompurify', ['NgDompurifyPipe'])
classes('@taiga-ui/kit', [
  'TuiAccordion',
  'TuiAvatar',
  'TuiButtonLoading',
  'TuiChevron',
  'TuiFade',
  'TuiProgressCircle',
  'TuiSkeleton',
])
classes('src/app/routes/portal/routes/services/pipes/install-progress.pipe', [
  'InstallingProgressPipe',
])
classes('src/app/services/hidden-updates.service', ['HiddenUpdatesService'])
classes('src/app/services/marketplace.service', ['MarketplaceService'])
classes('src/app/services/trusted-keys.service', ['TrustedKeysService'])
classes('../marketplace/services/alerts.service', ['MarketplaceAlertsService'])
classes('./updates.component', [], 'export default class Updates {}')
loadTypeScript(boundaries)

const { DialogService, Exver } = await import('@start9labs/shared')
const { HiddenUpdatesService } =
  await import('src/app/services/hidden-updates.service')
const { MarketplaceService } =
  await import('src/app/services/marketplace.service')
const { TrustedKeysService } =
  await import('src/app/services/trusted-keys.service')
const { MarketplaceAlertsService } =
  await import('../marketplace/services/alerts.service')
const { default: Updates } = await import('./updates.component')
const { UpdatesItemComponent } =
  await import('../src/app/routes/portal/routes/updates/item.component.ts')

const setInput = (input, value) => {
  const node = input[SIGNAL]
  node.applyValueToInputSignal(node, value)
}
const listing = version => ({
  id: 'example',
  version,
  flavor: null,
  releaseNotes: version,
})

test('expanded update notes follow both the displayed target and its registry', t => {
  const current = signal({ url: 'https://one.test/' })
  const requests = []
  const responses = []
  const injector = createEnvironmentInjector([
    { provide: ɵINJECTOR_SCOPE, useValue: 'root' },
    { provide: ɵChangeDetectionScheduler, useValue: { notify() {} } },
    { provide: DialogService, useValue: {} },
    { provide: HiddenUpdatesService, useValue: {} },
    { provide: MarketplaceAlertsService, useValue: {} },
    { provide: TrustedKeysService, useValue: { keys: signal({}) } },
    { provide: Exver, useValue: { getFlavor: () => null } },
    { provide: Updates, useValue: { current } },
    {
      provide: MarketplaceService,
      useValue: {
        getDetails$(...params) {
          requests.push(params)
          const response = new Subject()
          responses.push(response)
          return response
        },
      },
    },
  ])
  t.after(() => injector.destroy())
  const effects = injector.get(ɵEffectScheduler)
  const item = runInInjectionContext(injector, () => new UpdatesItemComponent())
  setInput(item.item, listing('2.0.0:0'))
  setInput(item.local, { stateInfo: { manifest: { version: '1.0.0:0' } } })
  effects.flush()
  assert.deepEqual(requests, [])
  item.expanded.set(true)
  effects.flush()
  assert.deepEqual(requests, [['example', '2.0.0:0', 'https://one.test/']])
  responses[0].next(listing('2.0.0:0'))
  assert.equal(item.releases()[0].version, '2.0.0:0')

  setInput(item.item, listing('3.0.0:0'))
  effects.flush()
  assert.deepEqual(requests.at(-1), ['example', '3.0.0:0', 'https://one.test/'])
  assert.equal(item.releases()[0].version, '3.0.0:0')
  responses[0].next(listing('2.0.0:0'))
  assert.equal(item.releases()[0].version, '3.0.0:0')

  current.set({ url: 'https://two.test/' })
  effects.flush()
  assert.deepEqual(requests.at(-1), ['example', '3.0.0:0', 'https://two.test/'])
  responses
    .at(-1)
    .next({ ...listing('3.0.0:0'), releaseNotes: 'Second registry' })
  assert.equal(item.releases()[0].releaseNotes, 'Second registry')
})
