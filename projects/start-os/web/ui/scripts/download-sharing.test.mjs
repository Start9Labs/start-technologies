import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { createEnvironmentInjector, runInInjectionContext } from '@angular/core'
import { PatchDB } from 'patch-db-client'
import { BehaviorSubject, Subject } from 'rxjs'
import { loadTypeScript } from './typescript-loader.mjs'

loadTypeScript(
  new Map([
    [
      '@start9labs/shared',
      'export class TaskService {} export class i18nPipe {} export class DocsLinkDirective {}',
    ],
    [
      '@taiga-ui/core',
      'export class TuiButton {} export class TuiCell {} export class TuiLoader {} export class TuiNotification {} export class TuiTitle {}',
    ],
    ['@taiga-ui/kit', 'export class TuiSwitch {}'],
    [
      '@taiga-ui/layout',
      'export class TuiCardLarge {} export class TuiHeader {}',
    ],
    ['src/app/services/api/embassy-api.service', 'export class ApiService {}'],
    [
      'src/app/routes/portal/components/interfaces/addresses/gateway/domain-health.service',
      'export class DomainHealthService {}',
    ],
    ['src/app/services/title.service', 'export class TitleDirective {}'],
    [
      'src/app/routes/portal/components/interfaces/interface.component',
      'export class InterfaceComponent {}',
    ],
    [
      'src/app/routes/portal/components/interfaces/interface.service',
      'export class InterfaceService {}',
    ],
  ]),
)
const { default: Downloads } =
  await import('src/app/routes/portal/routes/system/routes/downloads')
const { ApiService } = await import('src/app/services/api/embassy-api.service')
const { TaskService } = await import('@start9labs/shared')
const { GatewayService } = await import('src/app/services/gateway.service')
const { SharingAddresses } =
  await import('src/app/routes/portal/routes/system/routes/downloads/service')
const { InterfaceService } =
  await import('src/app/routes/portal/components/interfaces/interface.service')

const defaults = { downloads: false, seeding: false }
const downloadsEnabled = { downloads: true, seeding: false }
const config = {
  available: true,
  preview: null,
  interface: {
    id: 'bittorrent',
    addressInfo: { hostId: 'admin', internalPort: 6881 },
  },
}

function fixture(t, initial, setPreferences, readConfig = async () => config) {
  const updates = new Subject()
  const patch = new PatchDB(
    updates,
    new BehaviorSubject({
      id: 1,
      value: {
        serverInfo: {
          bittorrent: initial,
          statusInfo: { bittorrentWatcherFailed: false },
          network: {
            defaultOutbound: 'eth0',
            gateways: {
              eth0: { name: 'Home', ipInfo: null },
              wireguard2: { name: 'StartTunnel', ipInfo: null },
            },
            host: { bindings: {} },
          },
        },
        packageData: {},
      },
    }),
  )
  patch.start()
  const errors = []
  let reading
  const run = async operation => {
    try {
      await operation()
      return true
    } catch (error) {
      errors.push(error)
      return false
    }
  }
  const injector = createEnvironmentInjector([
    { provide: PatchDB, useValue: patch },
    {
      provide: ApiService,
      useValue: {
        rpc: {
          server: {
            bittorrent: {
              config: () => {
                const response = readConfig()
                reading = response.then(
                  () => undefined,
                  () => undefined,
                )
                return response
              },
              setPreferences,
            },
          },
        },
      },
    },
    {
      provide: TaskService,
      useValue: { run, runInline: operation => (reading = run(operation)) },
    },
    { provide: InterfaceService, useValue: { getServiceInterfaces: () => [] } },
    GatewayService,
    SharingAddresses,
  ])
  t.after(() => {
    injector.destroy()
    patch.stop()
  })
  const page = runInInjectionContext(injector, () => new Downloads())
  let revision = 1
  const observe = (value, path = '/serverInfo/bittorrent') =>
    updates.next([{ id: ++revision, patch: [{ op: 'replace', path, value }] }])
  return { page, errors, reading, observe, waitForConfig: () => reading }
}

for (const scenario of [
  {
    name: 'opt into downloads from disabled defaults',
    initial: defaults,
    preference: 'downloads',
    desired: true,
    observed: downloadsEnabled,
  },
  {
    name: 'disable downloads',
    initial: downloadsEnabled,
    preference: 'downloads',
    desired: false,
    observed: { downloads: false, seeding: false },
  },
  {
    name: 'enable seeding',
    initial: downloadsEnabled,
    preference: 'seeding',
    desired: true,
    observed: { downloads: true, seeding: true },
  },
  {
    name: 'disable seeding',
    initial: { downloads: true, seeding: true },
    preference: 'seeding',
    desired: false,
    observed: downloadsEnabled,
  },
  {
    name: 'normalized success after a concurrent parent disable',
    initial: downloadsEnabled,
    preference: 'seeding',
    desired: true,
    observed: { downloads: false, seeding: false },
  },
  {
    name: 'restore after failure',
    initial: downloadsEnabled,
    preference: 'seeding',
    desired: true,
    observed: downloadsEnabled,
    failure: true,
  },
  {
    name: 'restore newer enabled intent',
    initial: downloadsEnabled,
    preference: 'seeding',
    desired: true,
    observed: { downloads: true, seeding: true },
    failure: true,
  },
  {
    name: 'restore newer disabled intent',
    initial: { downloads: true, seeding: true },
    preference: 'downloads',
    desired: false,
    observed: { downloads: false, seeding: false },
    failure: true,
  },
]) {
  test(`software sharing: ${scenario.name}`, async t => {
    let resolve, reject
    const remote = new Promise((yes, no) => {
      resolve = yes
      reject = no
    })
    const requests = []
    const { page, observe, errors, reading } = fixture(
      t,
      scenario.initial,
      params => {
        requests.push(params)
        return remote
      },
    )
    await reading
    assert.deepEqual(page.preferences(), scenario.initial)
    assert.equal(page.controls.downloads.value, scenario.initial.downloads)
    assert.equal(page.controls.seeding.value, scenario.initial.seeding)
    assert.equal(page.controls.seeding.disabled, !scenario.initial.downloads)
    const control = page.controls[scenario.preference]
    control.setValue(scenario.desired, { emitEvent: false })
    const pending = page.save(scenario.preference, scenario.desired)
    assert.equal(page.saving(), true)
    assert.ok(Object.values(page.controls).every(control => control.disabled))
    observe(scenario.observed)
    if (scenario.failure) reject(new Error('save rejected'))
    else resolve(scenario.observed)
    await pending
    assert.equal(page.saving(), false)
    assert.deepEqual(page.preferences(), scenario.observed)
    assert.equal(control.value, scenario.observed[scenario.preference])
    assert.equal(page.controls.seeding.disabled, !scenario.observed.downloads)
    assert.deepEqual(requests, [{ [scenario.preference]: scenario.desired }])
    assert.equal(errors.length, scenario.failure ? 1 : 0)
  })
}

test('software sharing: delayed patches and later updates always reach the actual controls', async t => {
  const { page, reading, observe } = fixture(t, downloadsEnabled, async () => ({
    downloads: true,
    seeding: true,
  }))
  await reading
  page.controls.seeding.setValue(true, { emitEvent: false })
  await page.save('seeding', true)
  assert.equal(page.controls.seeding.value, false)
  observe({ downloads: true, seeding: true })
  assert.equal(page.controls.seeding.value, true)
  observe(defaults)
  assert.equal(page.controls.seeding.value, false)
})

test('software sharing: configured availability is separate from persisted intent', async t => {
  const { page, reading } = fixture(
    t,
    downloadsEnabled,
    async () => downloadsEnabled,
    async () => ({ ...config, available: false }),
  )
  assert.equal(page.sharing.config(), 'loading')
  await reading
  assert.equal(page.sharing.config().available, false)
  assert.deepEqual(page.preferences(), downloadsEnabled)
  assert.ok(Object.values(page.controls).every(control => control.disabled))
})

test('software sharing: watcher failure is observed without reloading configuration or changing intent', async t => {
  let reads = 0
  const initial = { downloads: true, seeding: true }
  const { page, reading, observe } = fixture(
    t,
    initial,
    async () => initial,
    async () => {
      reads++
      return config
    },
  )
  await reading
  assert.equal(page.watcherFailed(), false)
  observe(true, '/serverInfo/statusInfo/bittorrentWatcherFailed')
  assert.equal(page.watcherFailed(), true)
  assert.deepEqual(page.preferences(), initial)
  assert.equal(reads, 1)
})

test('software sharing: configuration failures remain retryable', async t => {
  let failed = true
  const { page, reading, errors, waitForConfig } = fixture(
    t,
    defaults,
    async () => defaults,
    async () => {
      if (failed) throw new Error('unavailable')
      return config
    },
  )
  await reading
  assert.equal(page.sharing.config(), 'error')
  assert.equal(errors.length, 0)
  failed = false
  page.sharing.refresh.next()
  await waitForConfig()
  assert.deepEqual(page.sharing.config(), config)
})

test('software sharing: network revisions refresh previews and discard older replies', async t => {
  const pending = []
  const state = fixture(
    t,
    defaults,
    async () => defaults,
    () => new Promise(resolve => pending.push(resolve)),
  )
  assert.equal(pending.length, 1)
  state.observe('wireguard2', '/serverInfo/network/defaultOutbound')
  assert.equal(pending.length, 2)
  pending[1]({ ...config, available: false })
  await state.waitForConfig()
  assert.equal(state.page.sharing.config().available, false)
  pending[0](config)
  await state.reading
  assert.equal(state.page.sharing.config().available, false)
})

test('software sharing: default gateway follows the authoritative network selection', async t => {
  const { page, reading, observe } = fixture(t, defaults, async () => defaults)
  await reading
  assert.equal(page.gateways.defaultOutboundGateway().name, 'Home')
  observe('wireguard2', '/serverInfo/network/defaultOutbound')
  assert.equal(page.gateways.defaultOutboundGateway().name, 'StartTunnel')
  observe(null, '/serverInfo/network/defaultOutbound')
  assert.equal(page.gateways.defaultOutboundGateway(), null)
})
