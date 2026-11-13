import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { createEnvironmentInjector, runInInjectionContext } from '@angular/core'
import { BehaviorSubject, of } from 'rxjs'
import { loadTypeScript } from './typescript-loader.mjs'

const boundaries = new Map()
const classes = (path, names, extra = '') =>
  boundaries.set(
    path,
    names.map(name => `export class ${name} {}`).join('\n') + extra,
  )
classes('patch-db-client', ['PatchDB'])
classes('@start9labs/shared', ['i18nPipe', 'TaskService'])
classes(
  '@start9labs/marketplace',
  ['StoreIconDirective', 'RegistryConnectionStatus', 'RegistryStatePipe'],
  `export { isRegistryLoading } from ${JSON.stringify(new URL('../../../../../shared-libs/ts-modules/marketplace/src/registry-state.ts', import.meta.url).href)};`,
)
classes(
  '@taiga-ui/core',
  ['TuiButton', 'TuiCell', 'TuiIcon', 'TuiNotification', 'TuiTitle'],
  ';export const TUI_ICON_START=Symbol();export const tuiButtonOptionsProvider=()=>[];',
)
classes('@taiga-ui/kit', [
  'TuiAvatar',
  'TuiBadgeNotification',
  'TuiFade',
  'TuiSkeleton',
])
classes('@ng-web-apis/platform', [], 'export const WA_IS_MOBILE=Symbol();')
for (const [file, name] of [
  ['hidden-updates', 'HiddenUpdatesService'],
  ['local-packages', 'LocalPackagesService'],
  ['marketplace', 'MarketplaceService'],
  ['notification', 'NotificationService'],
  ['os', 'OSService'],
  ['updates-refinement', 'UpdatesRefinementService'],
  ['title', 'TitleDirective'],
])
  classes(
    `src/app/services/${file}.service`,
    [name],
    file === 'updates-refinement' ? ';export const refinementKey=()=>"";' : '',
  )
classes('src/app/routes/portal/components/placeholder.component', [
  'PlaceholderComponent',
])
classes('src/app/routes/portal/components/table.component', ['TableComponent'])
classes('./item.component', ['UpdatesItemComponent'])
classes('./filter-updates.pipe', ['FilterUpdatesPipe'])
boundaries.set(
  '../routes/portal/routes/updates/filter-updates.pipe',
  'export class FilterUpdatesPipe { transform(packages) { return packages } }',
)
boundaries.set(
  'src/app/routes/portal/routes/system/routes/backups/conversion-state',
  'export const conversionLabel=()=>null;',
)
loadTypeScript(boundaries)
const { PatchDB } = await import('patch-db-client')
const { WA_IS_MOBILE } = await import('@ng-web-apis/platform')
const { HiddenUpdatesService } =
  await import('src/app/services/hidden-updates.service')
const { LocalPackagesService } =
  await import('src/app/services/local-packages.service')
const { MarketplaceService } =
  await import('src/app/services/marketplace.service')
const { NotificationService } =
  await import('src/app/services/notification.service')
const { OSService } = await import('src/app/services/os.service')
const { UpdatesRefinementService } =
  await import('src/app/services/updates-refinement.service')
const { FilterUpdatesPipe } =
  await import('../routes/portal/routes/updates/filter-updates.pipe')
const { BadgeService } = await import('../src/app/services/badge.service.ts')
const { default: Updates } =
  await import('../src/app/routes/portal/routes/updates/updates.component.ts')

function fixture(t, mobile = false) {
  const hosts = new BehaviorSubject([
    { url: 'https://one.test/', name: 'One', status: 'connected' },
    { url: 'https://two.test/', name: 'Two', status: 'connected' },
  ])
  const errors = new BehaviorSubject([])
  const refined = new BehaviorSubject({
    marketplace: { 'https://one.test/': { packages: [{ id: 'one' }] } },
    pending: new Set(),
  })
  const local = new BehaviorSubject({})
  const injector = createEnvironmentInjector([
    { provide: WA_IS_MOBILE, useValue: mobile },
    {
      provide: MarketplaceService,
      useValue: { registries$: hosts, requestErrors$: errors },
    },
    { provide: UpdatesRefinementService, useValue: { refined$: refined } },
    { provide: LocalPackagesService, useValue: local },
    { provide: HiddenUpdatesService, useValue: { effective$: of({}) } },
    { provide: NotificationService, useValue: { unreadCount$: of(0) } },
    { provide: OSService, useValue: { updateAvailable$: of(false) } },
    {
      provide: PatchDB,
      useValue: {
        watch$: (...path) => of(path.at(-1) === 'ntpSynced' ? true : {}),
      },
    },
    { provide: FilterUpdatesPipe, useValue: new FilterUpdatesPipe() },
  ])
  t.after(() => injector.destroy())
  return {
    hosts,
    errors,
    refined,
    make: type => runInInjectionContext(injector, () => new type()),
  }
}

test('registry errors replace the navigation count without dropping retained update rows', t => {
  const f = fixture(t)
  const badges = f.make(BadgeService)
  const counts = [],
    warnings = []
  const subscriptions = [
    badges.getCount('updates').subscribe(value => counts.push(value)),
    badges.getWarning('updates').subscribe(value => warnings.push(value)),
  ]
  t.after(() =>
    subscriptions.forEach(subscription => subscription.unsubscribe()),
  )
  assert.equal(counts.at(-1), 1)
  assert.equal(warnings.at(-1), false)
  f.errors.next(['https://two.test/'])
  assert.equal(counts.at(-1), 0)
  assert.equal(warnings.at(-1), true)
  f.refined.next({
    marketplace: {
      'https://one.test/': { packages: [{ id: 'one' }, { id: 'two' }] },
    },
    pending: new Set(),
  })
  assert.equal(counts.at(-1), 0)
  f.errors.next([])
  assert.equal(counts.at(-1), 2)
  assert.equal(warnings.at(-1), false)
})

test('registry availability revisions update the selected table without selecting the first registry', t => {
  const f = fixture(t)
  const page = f.make(Updates)
  page.selectedRegistry.set('https://two.test/')
  f.hosts.next([
    { url: 'https://one.test/', name: 'One', status: 'connected' },
    {
      url: 'https://two.test/',
      name: 'Renamed',
      status: 'error',
      error: 'unavailable',
    },
  ])
  assert.equal(page.current().url, 'https://two.test/')
  assert.equal(page.current().name, 'Renamed')
  assert.equal(page.current().status, 'error')
  f.hosts.next(f.hosts.value.map(row => ({ ...row, status: 'connecting' })))
  assert.equal(page.current().url, 'https://two.test/')
  assert.equal(page.current().status, 'connecting')
})

test('mobile registry selection stays empty until a row is selected and can return to the sidebar', t => {
  const f = fixture(t, true)
  const page = f.make(Updates)
  assert.equal(page.current(), null)
  page.selectedRegistry.set('https://two.test/')
  assert.equal(page.current().url, 'https://two.test/')
  page.selectedRegistry.set('')
  assert.equal(page.current(), null)
})
