import assert from 'node:assert/strict'
import { generateKeyPairSync, webcrypto } from 'node:crypto'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { loadTypeScript } from './typescript-loader.mjs'
import '@angular/compiler'
import {
  createEnvironmentInjector,
  runInInjectionContext,
  signal,
} from '@angular/core'
import { FormBuilder, NonNullableFormBuilder } from '@angular/forms'
import { BehaviorSubject, EMPTY, firstValueFrom, map, of, Subject } from 'rxjs'

const boundaries = new Map()
function boundary(specifier, names, extra = '') {
  boundaries.set(
    specifier,
    names.map(name => `export class ${name} {}`).join('\n') + '\n' + extra,
  )
}
boundary(
  '@start9labs/shared',
  [
    'TaskService',
    'DialogService',
    'DocsLinkDirective',
    'ErrorService',
    'i18nPipe',
    'i18nService',
    'LocalizePipe',
    'ConvertBytesPipe',
    'MarkdownPipe',
    'SafeLinksDirective',
    'RPCErrorDetails',
    'Exver',
    'ExverComparesPipe',
  ],
  'export const LANGUAGES = []; export const LANGUAGE_TO_CODE = {}; export const getKeyboardName = () => ""; export const getAllKeyboardsSorted = () => []; export const pauseFor = async () => {}; export const FALLBACK_ICON = "fallback.png"; export const defaultRegistries = { start9: "https://registry.test", community: "https://community.test" }; export const sameUrl = (a, b) => a === b; export const registryUrl = value => value;',
)
boundary(
  '@taiga-ui/core',
  [
    'TuiButton',
    'TuiLoader',
    'TuiScrollbar',
    'TuiAppearance',
    'TuiCell',
    'TuiDataList',
    'TuiDropdown',
    'TuiNotification',
    'TuiError',
    'TuiIcon',
    'TuiInput',
    'TuiTitle',
    'TuiGroup',
    'TuiCheckbox',
    'TuiHint',
  ],
  'export const tuiCellOptionsProvider = () => []; export const tuiValidationErrorsProvider = () => [];',
)
boundary('@taiga-ui/kit', [
  'TuiBadge',
  'TuiBadgeNotification',
  'TuiButtonLoading',
  'TuiButtonSelect',
  'TuiDataListWrapper',
  'TuiBlock',
  'TuiAvatar',
  'TuiRadioList',
  'TuiPassword',
  'TuiSegmented',
  'TuiSwitch',
  'TuiTextarea',
])
boundary(
  '@taiga-ui/cdk',
  ['TuiAnimated', 'TuiMapperPipe', 'TuiAutoFocus'],
  'export const tuiProvide = () => []; export const tuiMarkControlAsTouchedAndValidate = () => {};',
)
boundary('@taiga-ui/layout', ['TuiHeader', 'TuiForm'])
boundary('@taiga-ui/addon-table', ['TuiTable', 'TuiTableDirective'])
boundary('@taiga-ui/addon-mobile', ['TuiResponsiveDialog'])
boundary(
  '@taiga-ui/i18n',
  ['TuiLanguageSwitcherService'],
  'export const tuiLanguageSwitcher = () => [];',
)
boundary('@taiga-ui/dompurify', ['NgDompurifyPipe'])
boundary(
  '@ng-web-apis/platform',
  [],
  'export const WA_IS_MOBILE = Symbol("mobile"); export const WA_IS_ANDROID = Symbol("android"); export const WA_IS_IOS = Symbol("ios"); export const WA_REDUCED_MOTION = Symbol("reduced-motion"); export const isApple = () => false; export const isEdge = () => false; export const isFirefox = () => false; export const isSafari = () => false;',
)
boundary(
  '@ng-web-apis/common',
  [],
  'export const WA_WINDOW = Symbol("window"); export const WA_CRYPTO = Symbol("crypto"); export const WA_SESSION_STORAGE = Symbol("sessionStorage");',
)
boundary(
  '@taiga-ui/polymorpheus',
  [],
  `
import { inject, InjectionToken } from '@angular/core';
export const CONTEXT = new InjectionToken('dialog context');
export const injectContext = () => inject(CONTEXT);
export class PolymorpheusComponent { constructor(component) { this.component = component; } }
`,
)
boundary('src/app/services/api/embassy-api.service', ['ApiService'])
boundary('./embassy-api.service', ['ApiService'])
boundary('src/app/routes/portal/components/pkg-icon', ['PkgIconDirective'])
boundary('src/app/routes/portal/components/table.component', ['TableComponent'])
boundary(
  'src/app/services/patch-db/patch-db-source',
  [],
  'export const PATCH_CACHE = Symbol("patch cache");',
)
boundary('src/app/services/config.service', ['ConfigService'])
boundary('src/app/services/trusted-keys.service', ['TrustedKeysService'])
boundary('src/app/services/package-icon.service', ['PackageIconService'])
boundary('src/app/services/storage.service', ['StorageService'])
boundary('src/app/routes/portal/pipes/to-manifest', ['ToManifestPipe'])
boundary('../services/alerts.service', ['MarketplaceAlertsService'])
boundary('src/app/services/title.service', ['TitleDirective'])
boundary(
  'src/app/routes/portal/components/header/about.component',
  [],
  'export const ABOUT = {};',
)
for (const [file, name] of [
  ['keyboard-select.component', 'KeyboardSelectComponent'],
  ['server-name.dialog', 'ServerNameDialog'],
  ['snake.directive', 'SnakeDirective'],
]) {
  boundary(`./${file}`, [name])
}
boundaries.set(
  '@start9labs/marketplace',
  [
    'trust.ts',
    'types.ts',
    'services/abstract-marketplace.service.ts',
    'util/icon.ts',
  ]
    .map(
      file =>
        `export * from ${JSON.stringify(new URL(`../../../../../shared-libs/ts-modules/marketplace/src/${file}`, import.meta.url).href)};`,
    )
    .join('\n'),
)
loadTypeScript(boundaries)
const { SwUpdate } = await import('@angular/service-worker')
const { RefreshAlertComponent } =
  await import('src/app/components/refresh-alert.component')
const { PatchDB } = await import('patch-db-client')
const { OSService } = await import('src/app/services/os.service')
const { i18nService: ProductionI18nService } =
  await import('../../../../../shared-libs/ts-modules/shared/src/i18n/i18n.service.ts')
const { LocalizePipe: ProductionLocalizePipe } =
  await import('../../../../../shared-libs/ts-modules/shared/src/i18n/localize.pipe.ts')
const { I18N } =
  await import('../../../../../shared-libs/ts-modules/shared/src/i18n/i18n.providers.ts')
const { ApiService } = await import('src/app/services/api/embassy-api.service')
const { TaskService, DialogService, ErrorService, i18nPipe, i18nService } =
  await import('@start9labs/shared')
const { CONTEXT } = await import('@taiga-ui/polymorpheus')
const { WA_WINDOW } = await import('@ng-web-apis/common')
const { UiUpdateService } = await import('src/app/services/ui-update.service')
const { ConfigService } = await import('src/app/services/config.service')
const { TrustedKeysService } =
  await import('src/app/services/trusted-keys.service')
const { trustedOsUpdateSigners } =
  await import('src/app/utils/os-update-signers')
const { default: General } =
  await import('src/app/routes/portal/routes/system/routes/general/general.component')
const { SystemUpdateModal, UPDATE } =
  await import('src/app/routes/portal/routes/system/routes/general/update.component')

function deferred() {
  let resolve, reject
  const promise = new Promise((yes, no) => {
    resolve = yes
    reject = no
  })
  return { promise, resolve, reject }
}
const makeSigner = () =>
  generateKeyPairSync('ed25519').publicKey.export({
    type: 'spki',
    format: 'pem',
  })
const OS_UPDATE_SIGNERS = Array.from({ length: 3 }, makeSigner)
const untrustedSigner = makeSigner()
const trustedSigners = Object.fromEntries(
  OS_UPDATE_SIGNERS.map((key, i) => [
    key,
    {
      name: `Signer ${i}`,
      publicKeys: [key],
      removable: true,
      marketplace: true,
      os: true,
    },
  ]),
)
const squashfs = {
  x86_64: { signatures: { [OS_UPDATE_SIGNERS[0]]: 'signature' } },
}
const runningGitHash = 'a'.repeat(40)
const nextGitHash = 'b'.repeat(40)
const catalog = {
  '0.4.0.2': { releaseNotes: 'equal', squashfs },
  '0.4.0.4': { releaseNotes: 'newest', squashfs },
  '0.4.0.1': { releaseNotes: 'older', squashfs },
  '0.4.0.3': { releaseNotes: 'newer', squashfs },
}
const candidates = [
  { version: '0.4.0.4', notes: 'newest', squashfs },
  { version: '0.4.0.3', notes: 'newer', squashfs },
]
const state = version => ({
  serverInfo: {
    id: 'server-id',
    version,
    statusInfo: {},
    trustedKeys: trustedSigners,
  },
  ui: { startosRegistry: 'https://registry.test', snakeHighScore: 0 },
})
const tick = () => new Promise(resolve => setImmediate(resolve))
function fixture(t, cachedVersion, providers = []) {
  const source = new Subject()
  const cache = new BehaviorSubject(
    cachedVersion
      ? { id: 1, value: state(cachedVersion) }
      : { id: 0, value: {} },
  )
  const patch = new PatchDB(source, cache)
  patch.start()
  let revision = cachedVersion ? 1 : 0
  const requests = [],
    submissions = [],
    opened = [],
    alerts = [],
    tasks = [],
    confirmations = [],
    updateErrors = []
  let completed = 0
  const api = {
    rpc: {
      async gitInfo(params) {
        assert.deepEqual(params, {})
        return `${runningGitHash}\n`
      },
      registry: {
        os: {
          version: {
            get(params) {
              const response = deferred()
              requests.push({ params, ...response })
              return response.promise
            },
          },
        },
      },
      server: {
        async deviceInfo() {
          return { os: { platform: 'x86_64' } }
        },
        async update(params) {
          submissions.push(params)
          const error = updateErrors.shift()
          if (error) throw error
        },
      },
    },
  }
  const taskService = {
    run(fn) {
      const task = (async () => {
        try {
          await fn()
          return true
        } catch {
          return false
        }
      })()
      tasks.push(task)
      return task
    },
  }
  const injector = createEnvironmentInjector([
    { provide: PatchDB, useValue: patch },
    { provide: ApiService, useValue: api },
    { provide: TaskService, useValue: taskService },
    {
      provide: DialogService,
      useValue: {
        openComponent(content) {
          opened.push(content)
          return EMPTY
        },
        openAlert(message) {
          alerts.push(message)
          return EMPTY
        },
        openConfirm(options) {
          const response = new Subject()
          confirmations.push({ options, response })
          return response
        },
      },
    },
    { provide: i18nPipe, useValue: {} },
    { provide: i18nService, useValue: {} },
    { provide: WA_WINDOW, useValue: {} },
    { provide: ConfigService, useValue: {} },
    { provide: TrustedKeysService, useValue: {} },
    {
      provide: CONTEXT,
      useValue: {
        $implicit: {
          complete() {
            completed++
          },
        },
      },
    },
    { provide: OSService, useFactory: () => new OSService() },
    { provide: UiUpdateService, useFactory: () => new UiUpdateService() },
    ...providers,
  ])
  const os = injector.get(OSService)
  const availability = [],
    shown = []
  const subs = [
    os.updateAvailable$.subscribe(value => availability.push(value)),
    os.showUpdate$.subscribe(value => shown.push(value)),
  ]
  t.after(() => {
    subs.forEach(sub => sub.unsubscribe())
    injector.destroy()
    patch.stop()
  })
  return {
    os,
    patch,
    api,
    requests,
    submissions,
    opened,
    alerts,
    tasks,
    confirmations,
    updateErrors,
    availability,
    shown,
    get completed() {
      return completed
    },
    make: Class => runInInjectionContext(injector, () => new Class()),
    get: token => injector.get(token),
    live(version) {
      source.next([{ id: ++revision, value: state(version) }])
    },
    trust(signers) {
      source.next([
        {
          id: ++revision,
          patch: [
            {
              op: 'replace',
              path: '/serverInfo/trustedKeys',
              value: signers,
            },
          ],
        },
      ])
    },
    version(version) {
      source.next([
        {
          id: ++revision,
          patch: [
            { op: 'replace', path: '/serverInfo/version', value: version },
          ],
        },
      ])
    },
    async load(response = catalog) {
      const loading = os.loadOS()
      await tick()
      requests.at(-1).resolve(response)
      await loading
    },
  }
}

function uiFixture(t, { enabled = true, controlled = true } = {}) {
  const events = new Subject()
  const unrecoverable = new Subject()
  const checks = []
  const operations = []
  let reloads = 0
  const caches = new Map([
    ['ngsw:/:db:control', {}],
    ['ngsw:/:hash:assets:app:cache', {}],
    ['ngsw:/:1:data:icons:cache', {}],
    ['ngsw:/other/:db:control', {}],
    ['other-cache', {}],
  ])
  const registration = {
    scope: 'https://server.test/',
    async unregister() {
      operations.push('unregister')
    },
  }
  const win = {
    navigator: {
      serviceWorker: {
        controller: controlled ? {} : null,
        async getRegistration() {
          return registration
        },
      },
    },
    location: {
      reload() {
        reloads++
        operations.push('reload')
      },
    },
    caches: {
      async keys() {
        operations.push('keys')
        return [...caches.keys()]
      },
      async delete(name) {
        operations.push(name)
        return caches.delete(name)
      },
    },
  }
  const f = fixture(t, '0.4.0.4', [
    {
      provide: SwUpdate,
      useValue: {
        isEnabled: enabled,
        versionUpdates: events,
        unrecoverable,
        checkForUpdate() {
          const check = deferred()
          checks.push(check)
          return check.promise
        },
      },
    },
    {
      provide: ConfigService,
      useValue: { version: '0.4.0.4' },
    },
    { provide: WA_WINDOW, useValue: win },
  ])
  const ui = f.get(UiUpdateService)
  return {
    ...f,
    ui,
    events,
    unrecoverable,
    checks,
    operations,
    caches,
    registration,
    win,
    get reloads() {
      return reloads
    },
  }
}

const detected = { type: 'VERSION_DETECTED', version: { hash: 'new' } }
const ready = {
  type: 'VERSION_READY',
  currentVersion: { hash: 'old' },
  latestVersion: { hash: 'new' },
}
const noUpdate = { type: 'NO_NEW_VERSION_DETECTED', version: { hash: 'old' } }
const failed = {
  type: 'VERSION_INSTALLATION_FAILED',
  version: { hash: 'new' },
  error: 'hash mismatch',
}

test('an OS version mismatch tracks background installation without reloading the page', async t => {
  const f = uiFixture(t)
  assert.equal(f.ui.state().status, 'idle')
  assert.equal(f.checks.length, 0)
  f.version('0.4.0.5')
  assert.equal(f.ui.state().status, 'checking')
  assert.equal(f.checks.length, 1)
  await f.ui.check()
  assert.equal(f.checks.length, 1)
  assert.equal(f.ui.busy(), false)
  assert.equal(f.ui.phase(), 'syncing')
  assert.equal(f.reloads, 0)
  f.events.next(detected)
  assert.equal(f.ui.state().status, 'updating')
  assert.equal(f.ui.phase(), 'syncing')
  assert.equal(f.reloads, 0)
  f.events.next(ready)
  f.checks[0].resolve(true)
  await tick()
  assert.equal(f.ui.state().status, 'ready')
  assert.equal(f.reloads, 0)
  assert.deepEqual(f.tasks, [])
  f.ui.refresh()
  assert.equal(f.ui.state().status, 'reloading')
  assert.equal(f.reloads, 1)
})

for (const options of [{ enabled: false }, { controlled: false }]) {
  test(`without an active worker only a version mismatch offers immediate reload: ${JSON.stringify(options)}`, t => {
    const f = uiFixture(t, options)
    const component = f.make(RefreshAlertComponent)
    assert.equal(component.show(), false)
    f.version('0.4.0.5')
    assert.equal(f.ui.state().status, 'ready')
    assert.equal(f.checks.length, 0)
    assert.equal(component.show(), true)
    f.ui.refresh()
    assert.equal(f.reloads, 1)
    assert.deepEqual(f.tasks, [])
  })
}

test('a ready event received before version mismatch is retained and not undone by a later no-update event', t => {
  const f = uiFixture(t)
  const component = f.make(RefreshAlertComponent)
  f.events.next(ready)
  assert.equal(component.show(), true)
  f.version('0.4.0.5')
  f.events.next(noUpdate)
  assert.equal(f.ui.state().status, 'ready')
  assert.equal(f.checks.length, 0)
})

test('an outdated page can refresh an already cached update even when the check downloads nothing new', async t => {
  const f = uiFixture(t)
  f.version('0.4.0.5')
  f.events.next(noUpdate)
  f.checks[0].resolve(false)
  await tick()
  assert.equal(f.ui.state().status, 'ready')
  f.ui.refresh()
  assert.equal(f.reloads, 1)
})

test('a false check without a no-new-version event is retryable, not falsely ready', async t => {
  const f = uiFixture(t)
  f.version('0.4.0.5')
  f.checks[0].resolve(false)
  await tick()
  assert.deepEqual(f.ui.state(), { status: 'error', error: null })
  assert.equal(f.reloads, 0)
  f.ui.refresh()
  assert.equal(f.reloads, 0)
  const retry = f.ui.check()
  f.events.next(noUpdate)
  f.checks[1].resolve(false)
  await retry
  assert.equal(f.ui.state().status, 'ready')
  f.ui.refresh()
  assert.equal(f.reloads, 1)
})

test('no-new-version events do not announce an update when the running interface matches the server', t => {
  const f = uiFixture(t)
  const component = f.make(RefreshAlertComponent)
  f.events.next(noUpdate)
  assert.equal(f.ui.state().status, 'idle')
  assert.equal(component.show(), false)
})

for (const failure of ['installation', 'unrecoverable']) {
  test(`${failure} preserves the page and cannot be bypassed by Refresh`, t => {
    const f = uiFixture(t)
    const emit = () =>
      failure === 'installation'
        ? f.events.next(failed)
        : f.unrecoverable.next({
            type: 'UNRECOVERABLE_STATE',
            reason: 'missing cached file',
          })
    emit()
    emit()
    assert.equal(f.ui.state().status, 'error')
    assert.equal(f.ui.busy(), false)
    assert.equal(f.ui.phase(), 'error')
    f.ui.refresh()
    emit()
    assert.equal(f.reloads, 0)
    assert.equal(f.ui.state().status, 'error')
  })
}

for (const outcome of [
  'ready',
  'cached',
  'installation',
  'unrecoverable',
  'offline',
  'rejected',
]) {
  test(`sync outcome ${outcome} never consumes an earlier Refresh click`, async t => {
    const f = uiFixture(t)
    f.version('0.4.0.5')
    f.ui.refresh()
    assert.equal(f.ui.busy(), false)
    assert.equal(f.checks.length, 1)
    assert.equal(f.reloads, 0)
    if (['ready', 'installation', 'unrecoverable'].includes(outcome))
      f.events.next(detected)
    if (outcome === 'ready') f.events.next(ready)
    else if (outcome === 'cached') f.events.next(noUpdate)
    else if (outcome === 'installation') f.events.next(failed)
    else if (outcome === 'unrecoverable')
      f.unrecoverable.next({
        type: 'UNRECOVERABLE_STATE',
        reason: 'missing file',
      })
    if (outcome === 'rejected') f.checks[0].reject(new Error('check failed'))
    else f.checks[0].resolve(outcome === 'ready')
    await tick()
    assert.equal(f.reloads, 0)
    if (['ready', 'cached'].includes(outcome)) {
      assert.equal(f.ui.phase(), 'ready')
      f.ui.refresh()
      assert.equal(f.ui.state().status, 'reloading')
      assert.equal(f.ui.busy(), true)
      f.events.next(failed)
      f.ui.refresh()
      assert.equal(f.reloads, 1)
    } else {
      assert.equal(f.ui.phase(), 'error')
      f.ui.refresh()
      assert.equal(f.reloads, 0)
    }
  })
}

test('a retry syncs matching server/UI versions without reloading on no-new-version', async t => {
  const f = uiFixture(t)
  f.events.next(failed)
  assert.equal(f.ui.outdated(), false)
  void f.ui.check()
  f.ui.refresh()
  assert.equal(f.reloads, 0)
  f.events.next(noUpdate)
  f.checks[0].resolve(false)
  await tick()
  assert.equal(f.ui.state().status, 'ready')
  assert.equal(f.ui.phase(), 'ready')
  assert.equal(f.reloads, 0)
  f.ui.refresh()
  assert.equal(f.reloads, 1)
})

test('Refresh before sync completion never queues a reload', t => {
  const f = uiFixture(t)
  f.ui.refresh()
  assert.equal(f.checks.length, 0)
  assert.equal(f.reloads, 0)
  f.version('0.4.0.5')
  f.ui.refresh()
  f.events.next(detected)
  f.ui.refresh()
  f.events.next(ready)
  assert.equal(f.reloads, 0)
  f.ui.refresh()
  f.ui.refresh()
  assert.equal(f.reloads, 1)
})

test('a rejected check stays actionable without reloading the working UI, and can be retried', async t => {
  const f = uiFixture(t)
  f.version('0.4.0.5')
  f.checks[0].reject(new Error('offline'))
  await tick()
  assert.equal(f.reloads, 0)
  assert.match(f.ui.state().error, /offline/)
  const retry = f.ui.check()
  f.checks[1].reject(new Error('still offline'))
  await retry
  assert.equal(f.reloads, 0)
  assert.match(f.ui.state().error, /still offline/)
  const recovered = f.ui.check()
  f.checks[2].resolve(true)
  await recovered
  assert.equal(f.ui.state().status, 'ready')
})

test('a silent sync retains the cache-reset escape without offering an early refresh', async t => {
  const f = uiFixture(t)
  const component = f.make(RefreshAlertComponent)
  f.version('0.4.0.5')
  await tick()
  assert.equal(f.ui.state().status, 'checking')
  f.ui.refresh()
  assert.equal(f.ui.busy(), false)
  assert.equal(f.reloads, 0)
  assert.deepEqual(f.tasks, [])
  component.resetCache()
  assert.equal(f.confirmations[0].options.data.yes, 'Reset UI cache')
  f.confirmations[0].response.next(false)
  assert.deepEqual(f.operations, [])
  component.resetCache()
  f.confirmations[1].response.next(true)
  f.events.next(failed)
  await tick()
  assert.deepEqual(f.operations, [
    'unregister',
    'keys',
    'ngsw:/:db:control',
    'ngsw:/:hash:assets:app:cache',
    'ngsw:/:1:data:icons:cache',
    'reload',
  ])
  assert.deepEqual(
    [...f.caches.keys()],
    ['ngsw:/other/:db:control', 'other-cache'],
  )
  assert.equal(f.reloads, 1)
  f.checks[0].resolve(false)
  await tick()
  assert.equal(f.ui.state().status, 'reloading')
})

test('cache-reset failure preserves the UI and exposes the error without an automatic reload', async t => {
  const f = uiFixture(t)
  f.version('0.4.0.5')
  f.registration.unregister = async () => {
    throw new Error('browser refused reset')
  }
  await f.ui.resetCache()
  assert.equal(f.ui.state().status, 'error')
  assert.equal(f.ui.busy(), false)
  assert.match(f.ui.state().error, /browser refused reset/)
  assert.equal(f.reloads, 0)
  assert.equal(f.caches.size, 5)
})

test('syncing remains dismissible; only a completed sync can start non-dismissible navigation', async t => {
  const f = uiFixture(t)
  const component = f.make(RefreshAlertComponent)
  f.version('0.4.0.5')
  assert.equal(await firstValueFrom(component.canClose$), true)
  component.dismissed.set(true)
  assert.equal(component.show(), false)
  f.ui.refresh()
  assert.equal(await firstValueFrom(component.canClose$), true)
  assert.equal(component.show(), false)
  f.events.next(detected)
  assert.equal(component.show(), false)
  f.events.next(ready)
  assert.equal(component.show(), true)
  f.ui.refresh()
  assert.equal(await firstValueFrom(component.canClose$), false)
  component.dismissed.set(true)
  assert.equal(component.show(), true)
})

test('a confirmed cache reset blocks dismissal until navigation', async t => {
  const f = uiFixture(t)
  const component = f.make(RefreshAlertComponent)
  const unregister = deferred()
  f.registration.unregister = () => unregister.promise
  const resetting = f.ui.resetCache()
  assert.equal(f.ui.phase(), 'syncing')
  assert.equal(f.ui.busy(), true)
  assert.equal(await firstValueFrom(component.canClose$), false)
  unregister.resolve()
  await resetting
  assert.equal(f.reloads, 1)
})

test('dismissing an update dialog does not hide the next state', t => {
  const f = uiFixture(t)
  const component = f.make(RefreshAlertComponent)
  f.version('0.4.0.5')
  assert.equal(component.show(), true)
  component.dismissed.set(true)
  assert.equal(component.show(), false)
  f.events.next(ready)
  assert.equal(component.show(), true)
})

for (const liveBeforeResponse of [true, false]) {
  test(`cached older version is replaced by live current ${liveBeforeResponse ? 'before' : 'after'} registry response`, async t => {
    const f = fixture(t, '0.4.0.1')
    const loading = f.os.loadOS()
    await tick()
    assert.deepEqual(f.requests[0].params, {
      registry: 'https://registry.test',
      serverId: 'server-id',
    })
    if (liveBeforeResponse) f.live('0.4.0.2')
    f.requests[0].resolve({ '0.4.0.2': { releaseNotes: 'installed' } })
    await loading
    if (!liveBeforeResponse) {
      assert.equal(f.os.updateCandidates()[0].version, '0.4.0.2')
      f.live('0.4.0.2')
    }
    assert.deepEqual(f.os.updateCandidates(), [])
    assert.equal(f.availability.at(-1), false)
    assert.equal(f.shown.at(-1), false)
    assert.equal(f.requests.length, 1)
  })
}

test('without a cache, load waits for the first live state', async t => {
  const f = fixture(t)
  const loading = f.os.loadOS()
  await tick()
  assert.equal(f.requests.length, 0)
  assert.deepEqual(f.os.updateCandidates(), [])
  f.live('0.4.0.2')
  await tick()
  f.requests[0].resolve({ '0.4.0.2': { releaseNotes: 'installed' } })
  await loading
  assert.deepEqual(f.os.updateCandidates(), [])
  assert.equal(f.availability.at(-1), false)
})

test('an empty catalog clears availability and cannot be submitted', async t => {
  const f = fixture(t, '0.4.0.2')
  await f.load()
  await f.load({})
  assert.deepEqual(f.os.updateCandidates(), [])
  assert.equal(f.availability.at(-1), false)
  f.make(General).update()
  await f.make(SystemUpdateModal).update()
  assert.deepEqual(f.opened, [])
  assert.deepEqual(f.submissions, [])
  assert.equal(f.tasks.length, 0)
})

test('strictly newer candidates sort descending; refresh failure preserves catalog; version updates do not refetch', async t => {
  const f = fixture(t, '0.4.0.2')
  await f.load()
  assert.deepEqual(f.os.updateCandidates(), candidates)
  assert.equal(f.availability.at(-1), true)
  assert.equal(f.shown.at(-1), true)
  const refresh = f.os.loadOS()
  await tick()
  assert.equal(f.requests.length, 2)
  assert.deepEqual(f.os.updateCandidates(), candidates)
  f.requests[1].reject(new Error('registry unavailable'))
  await assert.rejects(refresh, /registry unavailable/)
  assert.deepEqual(f.os.updateCandidates(), candidates)
  f.version('0.4.0.3')
  assert.deepEqual(f.os.updateCandidates(), [candidates[0]])
  f.version('0.4.0.4')
  assert.deepEqual(f.os.updateCandidates(), [])
  assert.equal(f.requests.length, 2)
  await f.load({ '0.4.0.5': { releaseNotes: 'refreshed', squashfs } })
  assert.deepEqual(f.os.updateCandidates(), [
    { version: '0.4.0.5', notes: 'refreshed', squashfs },
  ])
})

test('a different image at the installed version is offered and submitted', async t => {
  const f = fixture(t, '0.4.0.2')
  await f.load({
    ...catalog,
    '0.4.0.2': { releaseNotes: 'next image', squashfs, gitHash: nextGitHash },
    '0.4.0.1': { releaseNotes: 'older image', squashfs, gitHash: nextGitHash },
  })
  assert.deepEqual(f.os.updateCandidates(), [
    ...candidates,
    { version: '0.4.0.2', notes: 'next image', squashfs },
  ])

  await f.load({
    '0.4.0.2': { releaseNotes: 'next image', squashfs, gitHash: nextGitHash },
  })
  assert.equal(f.availability.at(-1), true)
  assert.equal(f.shown.at(-1), true)
  f.make(General).update()
  assert.deepEqual(f.opened, [UPDATE])
  await f.make(SystemUpdateModal).update()
  assert.deepEqual(await Promise.all(f.tasks), [true])
  assert.equal(f.submissions[0].targetVersion, '=0.4.0.2')
  assert.equal(f.completed, 1)
})

for (const gitHash of [runningGitHash, null, undefined]) {
  test(`the installed version is excluded with matching or unknown registry hash: ${gitHash}`, async t => {
    const f = fixture(t, '0.4.0.2')
    await f.load({
      '0.4.0.2': { releaseNotes: 'installed', squashfs, gitHash },
    })
    assert.deepEqual(f.os.updateCandidates(), [])
    assert.equal(f.availability.at(-1), false)
    assert.equal(f.shown.at(-1), false)
  })
}

test('a refreshed same-version listing clears availability when its image matches the running build', async t => {
  const f = fixture(t, '0.4.0.2')
  await f.load({
    '0.4.0.2': { releaseNotes: 'next image', squashfs, gitHash: nextGitHash },
  })
  assert.equal(f.availability.at(-1), true)
  const modal = f.make(SystemUpdateModal)
  await f.load({
    '0.4.0.2': { releaseNotes: 'installed', squashfs, gitHash: runningGitHash },
  })
  assert.deepEqual(f.os.updateCandidates(), [])
  assert.equal(f.availability.at(-1), false)
  await modal.update()
  assert.deepEqual(f.submissions, [])
})

test('catalog publication waits for the running build hash', async t => {
  const f = fixture(t, '0.4.0.2')
  const hash = deferred()
  f.api.rpc.gitInfo = () => hash.promise
  const loading = f.os.loadOS()
  await tick()
  f.requests[0].resolve({
    '0.4.0.2': { releaseNotes: 'installed', squashfs, gitHash: runningGitHash },
  })
  await tick()
  assert.deepEqual(f.os.updateCandidates(), [])
  hash.resolve(`${runningGitHash}\n`)
  await loading
  assert.deepEqual(f.os.updateCandidates(), [])
  assert.deepEqual(f.availability, [false])
})

test('git-info failure preserves the last successful catalog', async t => {
  const f = fixture(t, '0.4.0.2')
  await f.load()
  f.api.rpc.gitInfo = async () => {
    throw new Error('git-info unavailable')
  }
  const refresh = f.os.loadOS()
  const rejected = assert.rejects(refresh, /git-info unavailable/)
  await tick()
  f.requests.at(-1).resolve({})
  await rejected
  assert.deepEqual(f.os.updateCandidates(), candidates)
})

test('release notes select the live UI language without refetching or changing registry data', async t => {
  const f = fixture(t, '0.4.0.2')
  const info = {
    releaseNotes: {
      en_US: 'English notes',
      pl_PL: 'Polskie informacje',
      de_DE: 'Deutsche Hinweise',
    },
    squashfs,
  }
  const original = structuredClone(info)
  await f.load({ '0.4.0.3': info })
  const locale = Object.create(ProductionI18nService.prototype)
  locale.language = 'polish'
  const injector = createEnvironmentInjector([
    { provide: ProductionI18nService, useValue: locale },
    { provide: I18N, useValue: signal(null) },
  ])
  t.after(() => injector.destroy())
  const pipe = runInInjectionContext(
    injector,
    () => new ProductionLocalizePipe(),
  )
  const notes = f.os.updateCandidates()[0].notes
  assert.equal(pipe.transform(notes), 'Polskie informacje')
  locale.language = 'german'
  assert.equal(pipe.transform(notes), 'Deutsche Hinweise')
  locale.language = 'french'
  assert.equal(pipe.transform(notes), 'English notes')
  locale.language = 'english'
  assert.equal(pipe.transform(notes), 'English notes')
  assert.equal(f.requests.length, 1)
  assert.deepEqual(info, original)

  await f.load({ '0.4.0.3': { releaseNotes: 'Legacy English', squashfs } })
  locale.language = 'polish'
  assert.equal(
    pipe.transform(f.os.updateCandidates()[0].notes),
    'Legacy English',
  )
})

test('General checks but does not open a modal without candidates', async t => {
  const f = fixture(t, '0.4.0.4')
  const general = f.make(General)
  general.onUpdate()
  await tick()
  f.requests[0].resolve(catalog)
  await Promise.all(f.tasks)
  await tick()
  assert.deepEqual(f.opened, [])
  assert.equal(f.alerts.length, 1)
  general.update()
  assert.deepEqual(f.opened, [])
  await f.make(SystemUpdateModal).update()
  assert.deepEqual(f.submissions, [])
  assert.equal(f.completed, 0)
})

test('General opens a modal after a successful manual check finds candidates', async t => {
  const f = fixture(t, '0.4.0.2')
  f.make(General).onUpdate()
  await tick()
  assert.deepEqual(f.opened, [])
  f.requests[0].resolve(catalog)
  await Promise.all(f.tasks)
  await tick()
  assert.deepEqual(f.opened, [UPDATE])
  assert.deepEqual(f.alerts, [])
})

test('General does not announce up-to-date after a failed manual check', async t => {
  const f = fixture(t, '0.4.0.2')
  f.make(General).onUpdate()
  await tick()
  f.requests[0].reject(new Error('registry unavailable'))
  await Promise.all(f.tasks)
  await tick()
  assert.deepEqual(f.opened, [])
  assert.deepEqual(f.alerts, [])
})

test('General opens the update modal and submission targets exactly the newest candidate', async t => {
  const f = fixture(t, '0.4.0.2')
  await f.load()
  f.make(General).onUpdate()
  assert.deepEqual(f.opened, [UPDATE])
  assert.equal(f.requests.length, 1)
  await f.make(SystemUpdateModal).update()
  await Promise.all(f.tasks)
  assert.equal(f.confirmations.length, 0)
  assert.deepEqual(f.submissions, [
    {
      targetVersion: '=0.4.0.4',
      registry: 'https://registry.test',
      progress: false,
      acceptSigners: { any: [{ signer: OS_UPDATE_SIGNERS[0] }] },
    },
  ])
  assert.equal(f.completed, 1)
})

test('a candidate disappearing after the modal opens prevents submission', async t => {
  const f = fixture(t, '0.4.0.2')
  await f.load()
  f.make(General).onUpdate()
  const modal = f.make(SystemUpdateModal)
  f.version('0.4.0.4')
  await modal.update()
  assert.equal(f.opened.length, 1)
  assert.deepEqual(f.submissions, [])
  assert.equal(f.completed, 0)
  assert.equal(f.tasks.length, 0)
})

for (const [name, key] of [
  ['configured signer 1', OS_UPDATE_SIGNERS[0]],
  ['configured signer 2', OS_UPDATE_SIGNERS[1]],
  ['configured signer 3', OS_UPDATE_SIGNERS[2]],
]) {
  test(`${name} listed as a signer skips the warning without verifying in the UI`, async t => {
    const f = fixture(t, '0.4.0.2')
    await f.load({
      '0.4.0.4': {
        releaseNotes: '',
        squashfs: {
          x86_64: {
            signatures: {
              [key]: 'left to downloader',
              [untrustedSigner]: 'another signature',
            },
          },
        },
      },
    })
    await f.make(SystemUpdateModal).update()
    await Promise.all(f.tasks)
    assert.equal(f.confirmations.length, 0)
    assert.equal(f.submissions.length, 1)
    assert.deepEqual(f.submissions[0].acceptSigners, { any: [{ signer: key }] })
    assert.equal(f.completed, 1)
  })
}

test('live trust-list changes remove default trust and allow a custom signer', async t => {
  const f = fixture(t, '0.4.0.2')
  await f.load()
  f.trust({})
  await f.make(SystemUpdateModal).update()
  await tick()
  assert.equal(f.alerts.length, 1)
  assert.equal(f.confirmations.length, 0)
  assert.deepEqual(f.submissions, [])
  await Promise.all(f.tasks)

  f.trust({
    custom: {
      name: 'My custom release signer',
      publicKeys: [untrustedSigner],
      removable: true,
      marketplace: false,
      os: true,
    },
  })
  await f.load({
    '0.4.0.4': {
      releaseNotes: '',
      squashfs: { x86_64: { signatures: { [untrustedSigner]: 'signature' } } },
    },
  })
  await f.make(SystemUpdateModal).update()
  await Promise.all(f.tasks)
  assert.equal(f.alerts.length, 1)
  assert.equal(f.confirmations.length, 0)
  assert.deepEqual(f.submissions[0].acceptSigners, {
    any: [{ signer: untrustedSigner }],
  })
})

test('trusted signer matching ignores PEM whitespace but rejects unrelated keys and unsigned metadata', () => {
  const key = OS_UPDATE_SIGNERS[0].replace(/\n/g, '\r\n')
  assert.deepEqual(trustedOsUpdateSigners([key], trustedSigners), [
    OS_UPDATE_SIGNERS[0],
  ])
  assert.deepEqual(
    trustedOsUpdateSigners(['unrelated key'], trustedSigners),
    [],
  )
  assert.deepEqual(trustedOsUpdateSigners([], trustedSigners), [])
  assert.deepEqual(trustedOsUpdateSigners([key], {}), [])
})

test('keyrings match PEM, Der64 and legacy base64url signers in both trust contexts', () => {
  const pem =
    '-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAgpcI3hOExv1njCt+zJoYBartdyk4SKiqSIy3+9tcpOI=\n-----END PUBLIC KEY-----\n'
  const der64 = 'MCowBQYDK2VwAyEAgpcI3hOExv1njCt+zJoYBartdyk4SKiqSIy3+9tcpOI='
  const base64url =
    'MCowBQYDK2VwAyEAgpcI3hOExv1njCt-zJoYBartdyk4SKiqSIy3-9tcpOI'
  for (const [publicKey, signer] of [
    [der64, pem],
    [pem, der64],
    [der64, base64url],
  ]) {
    const groups = {
      publisher: {
        name: 'Publisher',
        publicKeys: [publicKey],
        marketplace: true,
        os: true,
      },
    }
    assert.deepEqual(trustedOsUpdateSigners([signer], groups), [publicKey])
    for (const source of [
      { s9pks: [['x86_64', { signatures: { [signer]: 'signature' } }]] },
      { s9pks: [], signers: [signer] },
    ]) {
      assert.deepEqual(
        matchingTrustedKeys(packageSigners(source), groups, 'marketplace'),
        [{ id: 'publisher', name: 'Publisher', publicKeys: [publicKey] }],
      )
    }
  }
})

test('an untrusted update is blocked even when another platform or asset lists a trusted signer', async t => {
  const f = fixture(t, '0.4.0.2')
  await f.load({
    '0.4.0.4': {
      releaseNotes: '',
      squashfs: {
        x86_64: { signatures: { [untrustedSigner]: 'signature' } },
        aarch64: { signatures: { [OS_UPDATE_SIGNERS[0]]: 'signature' } },
      },
      iso: squashfs,
      authorized: ['drbonez'],
    },
  })
  await f.make(SystemUpdateModal).update()
  await tick()
  assert.equal(f.alerts.length, 1)
  assert.equal(f.confirmations.length, 0)
  assert.deepEqual(f.submissions, [])
  assert.equal(f.completed, 0)
  await Promise.all(f.tasks)
  assert.deepEqual(f.submissions, [])
})

for (const assets of [{ x86_64: { signatures: {} } }, {}]) {
  test(`missing signatures or platform metadata blocks submission: ${JSON.stringify(assets)}`, async t => {
    const f = fixture(t, '0.4.0.2')
    await f.load({ '0.4.0.4': { releaseNotes: '', squashfs: assets } })
    await f.make(SystemUpdateModal).update()
    await tick()
    assert.equal(f.alerts.length, 1)
    assert.equal(f.confirmations.length, 0)
    assert.deepEqual(f.submissions, [])
    await Promise.all(f.tasks)
  })
}

test('trust removed during device lookup blocks submission', async t => {
  const f = fixture(t, '0.4.0.2')
  await f.load()
  const device = deferred()
  f.api.rpc.server.deviceInfo = () => device.promise
  await f.make(SystemUpdateModal).update()
  f.trust({})
  device.resolve({ os: { platform: 'x86_64' } })
  await Promise.all(f.tasks)
  assert.equal(f.alerts.length, 1)
  assert.deepEqual(f.submissions, [])
  assert.equal(f.completed, 0)
})

test('a candidate disappearing during device lookup prevents submission', async t => {
  const f = fixture(t, '0.4.0.2')
  await f.load()
  const device = deferred()
  f.api.rpc.server.deviceInfo = () => device.promise
  await f.make(SystemUpdateModal).update()
  f.version('0.4.0.4')
  device.resolve({ os: { platform: 'x86_64' } })
  await Promise.all(f.tasks)
  assert.deepEqual(f.submissions, [])
  assert.equal(f.completed, 0)
})

test('device lookup and downloader failures stay in TaskService and leave the modal open', async t => {
  const f = fixture(t, '0.4.0.2')
  await f.load()
  f.updateErrors.push(new Error('invalid signature'))
  await f.make(SystemUpdateModal).update()
  assert.deepEqual(await Promise.all(f.tasks), [false])
  assert.equal(f.confirmations.length, 0)
  assert.equal(f.completed, 0)

  f.api.rpc.server.deviceInfo = async () => {
    throw new Error('unavailable')
  }
  await f.make(SystemUpdateModal).update()
  assert.deepEqual(await Promise.all(f.tasks), [false, false])
  assert.equal(f.submissions.length, 1)
  assert.equal(f.completed, 0)
})

const { BackupsBackupComponent } =
  await import('src/app/routes/portal/routes/system/routes/backups/backup.component')
const { BackupsRecoverComponent } =
  await import('src/app/routes/portal/routes/system/routes/backups/recover.component')
const { VolumeGrant } =
  await import('src/app/routes/portal/routes/system/routes/volumes/grant')
const { default: SystemVolumes } =
  await import('src/app/routes/portal/routes/system/routes/volumes/index')
const { getManifest, getCurrentManifest } =
  await import('src/app/utils/get-package-data')
const { Router } = await import('@angular/router')
const volumeA = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'
const volumeB = 'BCDEFGHIJKLMNOPQRSTUVWXYZ234567A'

function volumeFixture(t, packageData = {}) {
  const source = new Subject()
  const cache = new BehaviorSubject({
    id: 1,
    value: {
      packageData,
      sharedVolumes: {
        [volumeA]: {
          name: 'Photos',
          source: { kind: 'local' },
          grants: {},
          availability: 'available',
        },
        [volumeB]: {
          name: 'Music',
          source: { kind: 'cifs', hostname: 'nas.invalid', path: '/music' },
          grants: {},
          availability: 'unavailable',
        },
      },
    },
  })
  const patch = new PatchDB(source, cache)
  patch.start()
  const requests = [],
    results = [],
    errors = [],
    routes = [],
    grants = [],
    taskCalls = []
  let completed = 0
  const api = {
    rpc: {
      backup: {
        create: async params => {
          requests.push({ kind: 'backup', params })
        },
        volume: {
          restore: async (params, options) => {
            requests.push({ kind: 'volume', params, options })
            assert.ok(results.length)
            const result = results.shift()
            if (result instanceof Error) throw result
            return result
          },
        },
      },
      package: {
        backup: {
          restore: async params => {
            requests.push({ kind: 'package', params })
          },
        },
      },
      volumes: {
        listRetained: async () => ({ [volumeA]: 'Retained Photos' }),
        setGrant: async params => {
          grants.push(params)
        },
      },
    },
  }
  const injector = createEnvironmentInjector([
    { provide: PatchDB, useValue: patch },
    { provide: ApiService, useValue: api },
    { provide: DialogService, useValue: {} },
    {
      provide: TaskService,
      useValue: {
        async run(fn, message) {
          taskCalls.push(message)
          try {
            await fn()
            return true
          } catch (error) {
            errors.push(error)
            return false
          }
        },
      },
    },
    {
      provide: ErrorService,
      useValue: { handleError: error => errors.push(error) },
    },
    { provide: ConfigService, useValue: { version: '0.4.1' } },
    {
      provide: Router,
      useValue: {
        async navigate(route) {
          routes.push(route)
        },
      },
    },
    {
      provide: CONTEXT,
      useValue: {
        $implicit: {
          complete() {
            completed++
          },
        },
        data: {
          id: 'target',
          targetId: 'target',
          serverId: 'server',
          password: 'password',
          backupInfo: {
            packageBackups: {
              service: {
                title: 'Service',
                version: '1.0.0',
                osVersion: '0.4.0',
                timestamp: '2026-01-01T00:00:00Z',
              },
            },
            sharedVolumeBackups: {
              [volumeA]: {
                name: 'Photos',
                timestamp: '2026-01-01T00:00:00Z',
                generation: 'old-a',
              },
              [volumeB]: {
                name: 'Music',
                timestamp: '2026-01-01T00:00:00Z',
                generation: 'old-b',
              },
            },
          },
        },
      },
    },
  ])
  t.after(() => {
    injector.destroy()
    patch.stop()
  })
  return {
    api,
    requests,
    results,
    errors,
    routes,
    grants,
    taskCalls,
    get completed() {
      return completed
    },
    make: Class => runInInjectionContext(injector, () => new Class()),
  }
}

test('retained-volume page loading never starts a global task', async t => {
  const f = volumeFixture(t)
  const model = f.make(SystemVolumes)
  await Promise.resolve()
  assert.deepEqual(model.retained(), { [volumeA]: 'Retained Photos' })
  assert.deepEqual(f.taskCalls, [])
  assert.deepEqual(f.errors, [])
})

test('retained-volume page failures reach ErrorService without a global task', async t => {
  const f = volumeFixture(t)
  const error = { code: 0, message: 'unavailable' }
  f.api.rpc.volumes.listRetained = async () => {
    throw error
  }
  const model = f.make(SystemVolumes)
  await Promise.resolve()
  assert.deepEqual(model.retained(), {})
  assert.deepEqual(f.errors, [error])
  assert.deepEqual(f.taskCalls, [])
})

test('volume-only backup selection includes unavailable sources and sends an explicit empty service selection', async t => {
  const f = volumeFixture(t)
  const model = f.make(BackupsBackupComponent)
  assert.equal(model.hasSelection(), false)
  model.toggleSelectAll()
  assert.equal(model.hasSelection(), true)
  assert.equal(model.items().find(item => item.id === volumeB).disabled, false)
  await model.createBackup('password')
  assert.deepEqual(f.requests[0].params.packageIds, [])
  assert.deepEqual(
    new Set(f.requests[0].params.sharedVolumeIds),
    new Set([volumeA, volumeB]),
  )
  assert.equal(f.completed, 1)
  model.toggleSelectAll()
  assert.equal(model.hasSelection(), false)
})

test('backup toggle-all omits installing services without omitting volumes', t => {
  const f = volumeFixture(t, {
    installed: {
      stateInfo: {
        state: 'installed',
        manifest: { id: 'installed', title: 'Installed' },
      },
    },
    installing: {
      stateInfo: {
        state: 'installing',
        installingInfo: {
          newManifest: { id: 'installing', title: 'Installing' },
        },
      },
    },
  })
  const model = f.make(BackupsBackupComponent)
  model.toggleSelectAll()
  assert.deepEqual(
    new Set(
      model
        .items()
        .filter(item => item.checked)
        .map(item => item.id),
    ),
    new Set(['installed', volumeA, volumeB]),
  )
})

test('partial restore retry excludes submitted services and successful new volumes', async t => {
  const f = volumeFixture(t)
  const model = f.make(BackupsRecoverComponent)
  model.selected.set(new Set(model.options()))
  f.results.push({
    restored: { [volumeA]: 'new-a' },
    errors: { [volumeB]: 'offline backup target' },
  })
  await model.restore()
  assert.deepEqual(
    [...model.selected()].map(item => item.id),
    [volumeB],
  )
  assert.match(String(f.errors[0]), /Music: offline backup target/)
  assert.equal(
    model.options().find(option => option.kind === 'package').installed,
    true,
  )
  assert.equal(
    model.toMessage(model.options().find(option => option.kind === 'package'))
      .text,
    'Unavailable. Service is already installed.',
  )
  assert.ok(
    model.selected().has(model.options().find(option => option.id === volumeB)),
  )
  assert.equal(f.completed, 0)
  f.results.push({ restored: { [volumeB]: 'new-b' }, errors: {} })
  await model.restore()
  assert.deepEqual(
    f.requests.map(request => [request.kind, request.params.ids]),
    [
      ['package', ['service']],
      ['volume', [volumeB, volumeA]],
      ['volume', [volumeB]],
    ],
  )
  assert.deepEqual(f.requests[1].options, {
    timeout: 0,
    headers: { 'ngsw-bypass': 'true' },
  })
  assert.equal(model.selected().size, 0)
  assert.equal(f.completed, 1)
  assert.deepEqual(f.routes, [['/system/volumes']])
})

test('multiple volume restore errors have HTML line breaks', async t => {
  const f = volumeFixture(t)
  const model = f.make(BackupsRecoverComponent)
  model.selected.set(
    new Set(model.options().filter(option => option.kind === 'volume')),
  )
  f.results.push({
    restored: {},
    errors: { [volumeA]: 'first failure', [volumeB]: 'second failure' },
  })
  await model.restore()
  assert.equal(f.errors[0], 'Music: second failure<br />Photos: first failure')
  assert.equal(model.selected().size, 2)
})

test('restore transport failure retains volume selection and keeps the dialog open', async t => {
  const f = volumeFixture(t)
  const model = f.make(BackupsRecoverComponent)
  const selected = model.options().filter(option => option.kind === 'volume')
  model.selected.set(new Set(selected))
  f.results.push(new Error('connection lost'))
  await model.restore()
  assert.deepEqual([...model.selected()], selected)
  assert.equal(f.completed, 0)
  assert.deepEqual(f.routes, [])
})

test('failed service restore does not submit volumes or discard either selection', async t => {
  const f = volumeFixture(t)
  const model = f.make(BackupsRecoverComponent)
  model.selected.set(new Set(model.options()))
  f.api.rpc.package.backup.restore = async () => {
    throw new Error('authorization expired')
  }
  await model.restore()
  assert.equal(model.selected().size, 3)
  assert.deepEqual(f.requests, [])
  assert.equal(f.completed, 0)
})

test('grant choices map to the backend contract and pending resets after saving', async t => {
  const f = volumeFixture(t)
  const model = f.make(VolumeGrant)
  model.volumeId = () => volumeA
  model.packageId = () => 'service'
  for (const [choice, expected, index] of [
    ['Off', null, 0],
    ['Read', { readonly: true }, 1],
    ['Read/write', { readonly: false }, 2],
  ]) {
    await model.save(choice)
    assert.deepEqual(f.grants.at(-1), {
      id: volumeA,
      packageId: 'service',
      grant: expected,
    })
    assert.equal(model.pending(), false)
    assert.equal(model.activeIndex(), index)
  }
})

test('failed optimistic grant uses the latest observed grant rather than a stale rollback value', async t => {
  const f = volumeFixture(t)
  const model = f.make(VolumeGrant)
  model.volumeId = () => volumeA
  model.packageId = () => 'service'
  model.grant = signal({ readonly: true })
  const pending = deferred()
  f.api.rpc.volumes.setGrant = async () => {
    await pending.promise
    throw new Error('denied')
  }
  assert.equal(model.value(), 'Read')
  const saved = model.save('Off')
  assert.equal(model.pending(), true)
  model.grant.set({ readonly: false })
  pending.resolve()
  await saved
  assert.equal(model.value(), 'Read/write')
  assert.equal(model.activeIndex(), 2)
  assert.equal(model.pending(), false)
})

test('grant optimistic state resets when a reused row changes consumer or volume before the server patch arrives', async t => {
  const f = volumeFixture(t)
  const model = f.make(VolumeGrant)
  model.volumeId = signal(volumeA)
  model.packageId = signal('service')
  model.grant = signal(undefined)
  await model.save('Read/write')
  assert.equal(model.value(), 'Read/write')
  model.packageId.set('another-service')
  assert.equal(model.value(), 'Off')
  await model.save('Read')
  model.volumeId.set(volumeB)
  assert.equal(model.value(), 'Off')
})

test('grant capability reads the installed manifest throughout updates without changing candidate projections', () => {
  const installed = { sharedVolumes: true }
  const candidate = { sharedVolumes: false }
  const pkg = {
    stateInfo: {
      state: 'updating',
      manifest: installed,
      installingInfo: { newManifest: candidate },
    },
  }
  assert.equal(getCurrentManifest(pkg), installed)
  assert.equal(getManifest(pkg), candidate)
  pkg.stateInfo = { state: 'installed', manifest: candidate }
  assert.equal(getCurrentManifest(pkg), candidate)
  pkg.stateInfo = {
    state: 'installing',
    installingInfo: { newManifest: installed },
  }
  assert.equal(getCurrentManifest(pkg), installed)
})

test('production mocks support inspection, independent failed backups and new-local restore without public credentials', async t => {
  const { MockApiService } =
    await import('src/app/services/api/embassy-mock-api.service')
  const { Mock } = await import('src/app/services/api/api.fixures')
  const { mockPatchData } = await import('src/app/services/api/mock-patch')
  const { WA_CRYPTO, WA_SESSION_STORAGE } = await import('@ng-web-apis/common')
  const { PATCH_CACHE } =
    await import('src/app/services/patch-db/patch-db-source')
  const originalInfo = structuredClone(Mock.BackupInfo)
  const originalNotifications = [...Mock.Notifications]
  t.after(() => {
    Object.assign(Mock.BackupInfo, originalInfo)
    Mock.Notifications.splice(0, Infinity, ...originalNotifications)
  })
  const cache = new BehaviorSubject({
    id: 1,
    value: structuredClone(mockPatchData),
  })
  const injector = createEnvironmentInjector([
    { provide: WA_CRYPTO, useValue: webcrypto },
    { provide: WA_SESSION_STORAGE, useValue: {} },
    { provide: PATCH_CACHE, useValue: cache },
  ])
  const api = runInInjectionContext(injector, () => new MockApiService())
  const patch = new PatchDB(
    api.mockWsSource$.pipe(map(revision => [revision])),
    cache,
  )
  patch.start()
  t.after(() => {
    patch.stop()
    injector.destroy()
  })
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const local = await api.rpc.volumes.create({
    name: 'Photos',
    source: { kind: 'local' },
  })
  const remote = await api.rpc.volumes.create({
    name: 'Remote',
    source: {
      kind: 'cifs',
      hostname: 'nas.invalid',
      path: '/share',
      username: 'private-user',
      password: 'private-password',
    },
  })
  assert.match(local, /^[A-Z2-7]{32}$/)
  assert.match(remote, /^[A-Z2-7]{32}$/)
  assert.equal(cache.value.value.sharedVolumes[remote].availability, 'unknown')
  assert.doesNotMatch(
    JSON.stringify(cache.value.value.sharedVolumes),
    /private-user|private-password/,
  )
  assert.equal(
    (await api.rpc.volumes.inspect({ id: remote })).availability,
    'unavailable',
  )
  await api.rpc.volumes.rename({ id: local, name: 'Renamed photos' })
  Mock.BackupInfo.sharedVolumeBackups[remote] = {
    name: 'Previous remote copy',
    timestamp: '2026-01-01T00:00:00Z',
    generation: 'previous',
  }
  await api.rpc.backup.create({
    targetId: 'csgashbdjkasnd',
    packageIds: [],
    sharedVolumeIds: [local, remote],
    password: 'password',
    oldPassword: null,
  })
  t.mock.timers.tick(500)
  await tick()
  assert.equal(
    Mock.BackupInfo.sharedVolumeBackups[local].name,
    'Renamed photos',
  )
  assert.match(
    Mock.BackupInfo.sharedVolumeBackups[local].generation,
    /^[A-Z2-7]{32}$/,
  )
  assert.equal(
    Mock.BackupInfo.sharedVolumeBackups[remote].generation,
    'previous',
  )
  const report = Mock.Notifications[0].data
  assert.equal(report.sharedVolumes[local].error, null)
  assert.ok(report.sharedVolumes[remote].error)
  assert.deepEqual(report.packages, {})
  const restored = await api.rpc.backup.volume.restore({
    targetId: 'csgashbdjkasnd',
    serverId: '1234-5678-9876-5432',
    password: 'password',
    ids: [local],
  })
  assert.notEqual(restored.restored[local], local)
  assert.match(restored.restored[local], /^[A-Z2-7]{32}$/)
  assert.deepEqual(cache.value.value.sharedVolumes[restored.restored[local]], {
    name: 'Renamed photos',
    source: { kind: 'local' },
    grants: {},
    availability: 'available',
  })
  await api.rpc.volumes.remove({ id: local })
  assert.deepEqual(await api.rpc.volumes.listRetained({}), {
    [local]: 'Renamed photos',
  })
  await api.rpc.volumes.deleteData({ id: local })
  assert.deepEqual(await api.rpc.volumes.listRetained({}), {})
})

test('a candidate disappearing during the asynchronous UI registry lookup prevents submission', async t => {
  const f = fixture(t, '0.4.0.2')
  await f.load()
  const modal = f.make(SystemUpdateModal)
  const updating = modal.update()
  f.version('0.4.0.4')
  await updating
  assert.deepEqual(await Promise.all(f.tasks), [true])
  assert.deepEqual(f.submissions, [])
  assert.equal(f.completed, 0)
})

const { matchingTrustedKeys, packageSigners } =
  await import('@start9labs/marketplace')
const { parsePublicKeys, publicKeyPem } =
  await import('src/app/utils/public-keys')
const { MarketplaceControlsComponent: Controls } =
  await import('src/app/routes/portal/routes/marketplace/components/controls.component')
const { MarketplaceService } =
  await import('src/app/services/marketplace.service')

const pkg = {
  id: 'hello-world',
  version: '1.0.0:0',
  flavor: null,
  s9pks: [['x86_64', { signatures: { [OS_UPDATE_SIGNERS[0]]: 'signature' } }]],
}

test('PEM form parsing canonicalizes multiple keys and rejects empty, private, non-Ed25519, and mixed input', () => {
  const first = OS_UPDATE_SIGNERS[0]
  const second = OS_UPDATE_SIGNERS[1]
  assert.deepEqual(
    parsePublicKeys(` ${first.replace(/\n/g, ' ')}\n${second}\n${first}`),
    [first, second],
  )
  const { privateKey } = generateKeyPairSync('ed25519')
  const rsa = generateKeyPairSync('rsa', {
    modulusLength: 1024,
  }).publicKey.export({ type: 'spki', format: 'pem' })
  for (const invalid of [
    '',
    'garbage',
    first + 'trailing junk',
    first.slice(0, -15),
    rsa,
    privateKey.export({ type: 'pkcs8', format: 'pem' }),
  ]) {
    assert.throws(() => parsePublicKeys(invalid))
  }
  const raw = generateKeyPairSync('ed25519')
    .publicKey.export({ type: 'spki', format: 'der' })
    .subarray(-32)
  assert.deepEqual(parsePublicKeys(publicKeyPem(raw)), [publicKeyPem(raw)])
})

test('GPG form input preserves armored exports for backend validation alongside PEM keys', () => {
  const armored = readFileSync(
    new URL(
      '../../../../../shared-libs/crates/start-core/src/sign/fixtures/ed25519-signers.asc',
      import.meta.url,
    ),
    'utf8',
  ).trim()
  assert.deepEqual(
    parsePublicKeys(`${OS_UPDATE_SIGNERS[0]}\n${armored}\n${armored}`),
    [OS_UPDATE_SIGNERS[0], armored],
  )
  assert.throws(() =>
    parsePublicKeys(
      armored.replaceAll('PGP PUBLIC KEY BLOCK', 'PGP PRIVATE KEY BLOCK'),
    ),
  )
})

test('package signers select the install asset or the verified sideload signer, not other assets', () => {
  assert.deepEqual(
    packageSigners({
      ...pkg,
      s9pks: [
        ...pkg.s9pks,
        ['aarch64', { signatures: { [untrustedSigner]: '' } }],
      ],
    }),
    [OS_UPDATE_SIGNERS[0]],
  )
  assert.deepEqual(packageSigners({ s9pks: [], signers: [untrustedSigner] }), [
    untrustedSigner,
  ])
  assert.deepEqual(packageSigners({ s9pks: [] }), [])
})

test('named multi-key signer matching follows the selected trust context and deduplicates each identity', () => {
  const groups = {
    publisher: {
      name: 'Publisher',
      publicKeys: OS_UPDATE_SIGNERS,
      os: false,
      marketplace: true,
    },
  }
  assert.deepEqual(
    matchingTrustedKeys(
      [OS_UPDATE_SIGNERS[0], OS_UPDATE_SIGNERS[1]],
      groups,
      'marketplace',
    ),
    [
      {
        id: 'publisher',
        name: 'Publisher',
        publicKeys: OS_UPDATE_SIGNERS.slice(0, 2),
      },
    ],
  )
  assert.deepEqual(matchingTrustedKeys(OS_UPDATE_SIGNERS, groups, 'os'), [])
  groups.publisher.os = true
  groups.publisher.marketplace = false
  assert.deepEqual(
    matchingTrustedKeys(OS_UPDATE_SIGNERS, groups, 'marketplace'),
    [],
  )
  assert.equal(matchingTrustedKeys(OS_UPDATE_SIGNERS, groups, 'os').length, 1)
})

function controlsFixture({
  sideload = false,
  trusted = false,
  confirmation = EMPTY,
} = {}) {
  const calls = [],
    prompts = []
  const instance = {
    file: () => (sideload ? new File(['data'], 'test.s9pk') : undefined),
    trusted: () => (trusted ? [{ name: 'Publisher' }] : []),
    blocked: () => !sideload && !trusted,
    dialogs: {
      openConfirm(options) {
        prompts.push(options)
        return confirmation
      },
    },
    localPkg: () => undefined,
    pkg: () => pkg,
    alerts: { alertPreDownload: async () => true },
    marketplace: { currentRegistryUrl$: of('https://registry.test') },
    installOrUpload: async (...params) => {
      calls.push(params)
    },
  }
  return {
    instance,
    calls,
    prompts,
    run: () => Controls.prototype.tryInstall.call(instance),
  }
}

test('untrusted marketplace installs cannot enter the install flow or ask for an override', async () => {
  const f = controlsFixture()
  await f.run()
  assert.deepEqual(f.calls, [])
  assert.deepEqual(f.prompts, [])
})

for (const confirmation of [EMPTY, of(false)]) {
  test('dismissing or cancelling an untrusted sideload never starts an upload', async () => {
    const f = controlsFixture({ sideload: true, confirmation })
    await f.run()
    assert.equal(f.prompts.length, 1)
    assert.deepEqual(f.calls, [])
  })
}

test('only a confirmed untrusted sideload receives the trust override', async () => {
  const untrusted = controlsFixture({ sideload: true, confirmation: of(true) })
  await untrusted.run()
  assert.deepEqual(untrusted.calls, [[null, true]])
  const trusted = controlsFixture({ sideload: true, trusted: true })
  await trusted.run()
  assert.deepEqual(trusted.calls, [[null, false]])
  assert.deepEqual(trusted.prompts, [])
  const marketplace = controlsFixture({ trusted: true })
  await marketplace.run()
  assert.deepEqual(marketplace.calls, [['https://registry.test', false]])
})

test('the upload RPC receives the confirmed override and failed setup does not navigate', async () => {
  const calls = []
  const file = new File(['data'], 'test.s9pk')
  const instance = {
    file: () => file,
    tasks: {
      run: async work => {
        await work()
        return true
      },
    },
    api: {
      rpc: {
        package: {
          sideload: async params => {
            calls.push(params)
            return { upload: 'upload-guid' }
          },
        },
      },
      uploadFile: async (guid, uploaded) => {
        assert.equal(guid, 'upload-guid')
        assert.equal(uploaded, file)
      },
    },
  }
  assert.equal(await Controls.prototype.upload.call(instance, true), true)
  assert.deepEqual(calls, [{ trust: true }])
  let navigated = false
  await Controls.prototype.installOrUpload.call(
    {
      file: () => file,
      upload: async () => false,
      router: {
        navigate() {
          navigated = true
        },
      },
    },
    null,
    true,
  )
  assert.equal(navigated, false)
})

test('marketplace dispatch delegates trust enforcement to the server without an override', async () => {
  const submissions = []
  const instance = {
    api: {
      rpc: {
        package: {
          install: async params => {
            submissions.push(params)
          },
        },
      },
    },
  }
  await MarketplaceService.prototype.installPackage.call(
    instance,
    pkg,
    'https://registry.test',
  )
  assert.deepEqual(submissions, [
    { id: pkg.id, version: pkg.version, registry: 'https://registry.test' },
  ])
})

const { default: TrustedKeyrings } =
  await import('src/app/routes/portal/routes/system/routes/trusted-keys')
const { EditTrustedKeyDialog } =
  await import('src/app/components/trusted-keys/edit')
const { AddTrustedKeyDialog } =
  await import('src/app/components/trusted-keys/form')
test('built-in keyrings precede custom entries regardless of labels and input order', () => {
  const custom = Object.values(trustedSigners)[0]
  const entries = [
    { key: '00000000-0000-4000-8000-000000000000', value: custom },
    {
      key: 'start9-alpha',
      value: {
        ...custom,
        name: 'A renamed Alpha',
        removable: false,
        marketplace: false,
        os: false,
      },
    },
    { key: 'ffffffff-ffff-4fff-8fff-ffffffffffff', value: custom },
    {
      key: 'start9',
      value: { ...custom, name: 'Z renamed Start9', removable: false },
    },
  ]
  for (const input of [entries, [...entries].reverse()]) {
    assert.deepEqual(
      [...input]
        .sort(TrustedKeyrings.prototype.compareKeyrings)
        .map(x => x.key),
      [
        'start9',
        'start9-alpha',
        '00000000-0000-4000-8000-000000000000',
        'ffffffff-ffff-4fff-8fff-ffffffffffff',
      ],
    )
  }
})

function keyringDialogFixture(t, component, keyring, reject = false) {
  const mutations = []
  const closed = []
  const context = {
    data: { id: 'publisher', keyring },
    $implicit: { complete: () => closed.push(true) },
  }
  const mutate = async params => {
    mutations.push(params)
    if (reject) throw new Error('offline')
  }
  const injector = createEnvironmentInjector(
    [
      {
        provide: NonNullableFormBuilder,
        useValue: new FormBuilder().nonNullable,
      },
      { provide: CONTEXT, useValue: context },
      {
        provide: ApiService,
        useValue: {
          rpc: { server: { trustedKeys: { update: mutate, add: mutate } } },
        },
      },
      {
        provide: TaskService,
        useValue: {
          run: async work => {
            try {
              await work()
              return true
            } catch {
              return false
            }
          },
        },
      },
    ],
    null,
  )
  t.after(() => injector.destroy())
  return {
    instance: runInInjectionContext(injector, () => new component()),
    mutations,
    closed,
  }
}

test('keyring editing disables built-in names without disabling either trust selection', t => {
  for (const name of ['Start9', 'Start9 Alpha']) {
    const { instance } = keyringDialogFixture(t, EditTrustedKeyDialog, {
      name,
      removable: false,
      marketplace: true,
      os: false,
    })
    assert.equal(instance.form.controls.name.disabled, true)
    assert.equal(instance.form.controls.marketplace.enabled, true)
    assert.equal(instance.form.controls.os.enabled, true)
  }
  const { instance } = keyringDialogFixture(t, EditTrustedKeyDialog, {
    name: 'Publisher',
    removable: true,
    marketplace: true,
    os: false,
  })
  assert.equal(instance.form.controls.name.enabled, true)
})

test('keyring edit submits name and both trust selections in one operation', async t => {
  const f = keyringDialogFixture(t, EditTrustedKeyDialog, {
    name: 'Publisher',
    removable: true,
    marketplace: true,
    os: false,
  })
  f.instance.form.setValue({ name: 'Renamed', marketplace: false, os: true })
  await f.instance.save()
  assert.deepEqual(f.mutations, [
    { id: 'publisher', name: 'Renamed', marketplace: false, os: true },
  ])
  assert.deepEqual(f.closed, [true])
  assert.equal(f.instance.saving(), false)
})

test('failed keyring edits keep their form values and dialog open', async t => {
  const f = keyringDialogFixture(
    t,
    EditTrustedKeyDialog,
    { name: 'Publisher', removable: true, marketplace: true, os: false },
    true,
  )
  f.instance.form.setValue({ name: 'Renamed', marketplace: false, os: true })
  await f.instance.save()
  assert.deepEqual(f.instance.form.getRawValue(), {
    name: 'Renamed',
    marketplace: false,
    os: true,
  })
  assert.deepEqual(f.closed, [])
  assert.equal(f.instance.saving(), false)
})

test('new System keyrings default to package trust only', async t => {
  const f = keyringDialogFixture(t, AddTrustedKeyDialog)
  f.instance.form.setValue({
    name: 'Publisher',
    publicKeys: OS_UPDATE_SIGNERS[0],
  })
  await f.instance.save()
  assert.deepEqual(f.mutations, [
    {
      name: 'Publisher',
      publicKeys: [OS_UPDATE_SIGNERS[0]],
      context: 'marketplace',
    },
  ])
  assert.deepEqual(f.closed, [true])
})
