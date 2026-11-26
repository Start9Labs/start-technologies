import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import {
  createEnvironmentInjector,
  runInInjectionContext,
  ɵChangeDetectionScheduler,
  ɵEffectScheduler,
  ɵINJECTOR_SCOPE,
} from '@angular/core'
import { SIGNAL } from '@angular/core/primitives/signals'
import { convertToParamMap, Router } from '@angular/router'
import { Observable, of } from 'rxjs'
import { loadTypeScript } from './typescript-loader.mjs'

const boundaries = new Map()
const classes = (path, names, extra = '') =>
  boundaries.set(
    path,
    names.map(name => `export class ${name} {}`).join('\n') + extra,
  )
classes('@start9labs/shared', [
  'DialogService',
  'EmptyPipe',
  'Exver',
  'i18nPipe',
  'MARKDOWN',
])
classes('@taiga-ui/core', [
  'TuiButton',
  'TuiLoader',
  'TuiNotification',
  'TuiTitle',
])
classes('@taiga-ui/kit', ['TuiAvatar', 'TuiFade', 'TuiSkeleton'])
classes(
  '@taiga-ui/layout',
  ['TuiHeader'],
  'export const tuiCardOptionsProvider = () => []; export const tuiHeaderOptionsProvider = () => [];',
)
for (const [file, name] of [
  ['about', 'MarketplaceAboutComponent'],
  ['dependencies', 'MarketplaceDependenciesComponent'],
  ['flavors', 'MarketplaceFlavorsComponent'],
  ['links', 'MarketplaceLinksComponent'],
  ['release-notes', 'MarketplaceReleaseNotesComponent'],
])
  classes(`./${file}.component`, [name])
classes('./icon.directive', ['MarketplaceIconDirective'])
classes('../services/abstract-marketplace.service', [
  'AbstractMarketplaceService',
])
loadTypeScript(boundaries)
const { DialogService, Exver } = await import('@start9labs/shared')
const { Exver: ProductionExver } =
  await import('../../../../../shared-libs/ts-modules/shared/src/services/exver.service.ts')
const { AbstractMarketplaceService } =
  await import('../services/abstract-marketplace.service')
const { MarketplacePreviewComponent } =
  await import('../../../../../shared-libs/ts-modules/marketplace/src/components/preview.component.ts')

const setInput = (input, value) => {
  const node = input[SIGNAL]
  node.applyValueToInputSignal(node, value)
}
const listing = (version = '1.0.0:0') => ({
  id: 'alternate',
  version,
  flavor: null,
  title: 'Alternate',
  satisfies: [],
  satisfiesPackages: { bitcoin: ['26.0.0:0'] },
  description: { short: 'Short' },
  s9pks: [],
})
const details = version => ({
  ...listing(version),
  otherVersions: {},
  dependencyMetadata: {},
})

function preview(
  t,
  params = {
    provides: 'bitcoin',
    requirePkg: 'bitcoin',
    requireRange: '>=26.0.0',
  },
) {
  const requests = [],
    responses = []
  let releases = 0,
    destroyed = false
  const injector = createEnvironmentInjector([
    { provide: ɵINJECTOR_SCOPE, useValue: 'root' },
    { provide: ɵChangeDetectionScheduler, useValue: { notify() {} } },
    { provide: DialogService, useValue: {} },
    { provide: Exver, useValue: new ProductionExver() },
    {
      provide: Router,
      useValue: {
        routerState: { root: { queryParamMap: of(convertToParamMap(params)) } },
        lastSuccessfulNavigation: () => null,
      },
    },
    {
      provide: AbstractMarketplaceService,
      useValue: {
        currentRegistry$: of({ packages: [] }),
        getPackage$(...args) {
          requests.push(args)
          return new Observable(subscriber => {
            responses.push(subscriber)
            return () => releases++
          })
        },
      },
    },
  ])
  const component = runInInjectionContext(
    injector,
    () => new MarketplacePreviewComponent(),
  )
  setInput(component.pkgId, 'alternate')
  setInput(component.listing, listing())
  const flush = () => injector.get(ɵEffectScheduler).flush()
  const destroy = () => {
    if (!destroyed) {
      destroyed = true
      injector.destroy()
    }
  }
  t.after(destroy)
  flush()
  return {
    component,
    requests,
    responses,
    flush,
    destroy,
    releases: () => releases,
  }
}

test('a filtered listing is available immediately and resolves its matching physical release', t => {
  const f = preview(t)
  assert.equal(f.component.placeholder().version, '1.0.0:0')
  assert.equal(f.component.pkg(), null)
  assert.deepEqual(f.requests, [['alternate', '1.0.0:0', null]])
  f.responses[0].next(details('1.0.0:0'))
  assert.equal(f.component.pkg().version, '1.0.0:0')
})

test('a listing with unrelated claims cannot become the requirement placeholder', t => {
  const f = preview(t)
  setInput(f.component.listing, {
    ...listing(),
    satisfiesPackages: { other: ['26.0.0:0'] },
  })
  assert.equal(f.component.placeholder(), null)
})

test('default previews follow the listing version without refetching for title-only changes', t => {
  const f = preview(t)
  setInput(f.component.listing, { ...listing(), title: 'Renamed' })
  f.flush()
  assert.equal(f.requests.length, 1)
  setInput(f.component.listing, listing('2.0.0:0'))
  f.flush()
  assert.deepEqual(f.requests.at(-1), ['alternate', '2.0.0:0', null])
  assert.equal(f.releases(), 1)
  f.responses[0].next(details('1.0.0:0'))
  assert.equal(f.component.pkg(), null)
  f.responses[1].next(details('2.0.0:0'))
  assert.equal(f.component.pkg().version, '2.0.0:0')
})

test('an explicit version remains selected when the listing changes', t => {
  const f = preview(t)
  f.component.selectedVersion.set('0.5.0:0')
  f.flush()
  assert.deepEqual(f.requests.at(-1), ['alternate', '0.5.0:0', null])
  assert.equal(f.component.placeholder(), null)
  setInput(f.component.listing, listing('2.0.0:0'))
  f.flush()
  assert.equal(f.component.selectedVersion(), '0.5.0:0')
  assert.equal(f.requests.at(-1)[1], '0.5.0:0')
})
