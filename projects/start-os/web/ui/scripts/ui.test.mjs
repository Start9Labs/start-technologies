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
import { FormBuilder, NonNullableFormBuilder, Validators } from '@angular/forms'
import {
  BehaviorSubject,
  EMPTY,
  firstValueFrom,
  map,
  of,
  Subject,
  switchMap,
} from 'rxjs'

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
    'EmptyPipe',
    'MARKDOWN',
    'CopyService',
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
    'LeafProgressPipe',
  ],
  'export const LANGUAGES = []; export const LANGUAGE_TO_CODE = {}; export const getKeyboardName = () => ""; export const getAllKeyboardsSorted = () => []; export const pauseFor = async () => {}; export const FALLBACK_ICON = "fallback.png"; export const defaultRegistries = { start9: "https://registry.test", community: "https://community.test" }; export const convertBytes = value => String(value); export const getErrorMessage = error => error.message; export const leafProgress = progress => progress; ' +
    `export { sameUrl, registryUrl } from ${JSON.stringify(new URL('../../../../../shared-libs/ts-modules/shared/src/util/misc.util.ts', import.meta.url).href)};` +
    `export { legacyDatabaseSubscription } from ${JSON.stringify(new URL('../../../../../shared-libs/ts-modules/shared/src/services/database-subscription.ts', import.meta.url).href)};` +
    `export { withAbortSignal } from ${JSON.stringify(new URL('../../../../../shared-libs/ts-modules/shared/src/util/with-abort.ts', import.meta.url).href)};`,
)
boundary(
  '@taiga-ui/core',
  [
    'TuiButton',
    'TuiButtonX',
    'TuiLoader',
    'TuiScrollbar',
    'TuiAppearance',
    'TuiCell',
    'TuiDataList',
    'TuiDropdown',
    'TuiNotification',
    'TuiNotificationService',
    'TuiTextfield',
    'TuiError',
    'TuiIcon',
    'TuiInput',
    'TuiTitle',
    'TuiGroup',
    'TuiCheckbox',
    'TuiHint',
    'TuiLink',
    'TuiExpand',
  ],
  'export const TUI_ICON_START = Symbol("iconStart"); export const tuiButtonOptionsProvider = () => []; export const TUI_BREAKPOINT = Symbol("breakpoint"); export const tuiCellOptionsProvider = () => []; export const tuiValidationErrorsProvider = () => []; export const tuiIconStart = value => value;',
)
boundary(
  '@taiga-ui/kit',
  [
    'TuiAvatar',
    'TuiBadge',
    'TuiBadgeNotification',
    'TuiButtonLoading',
    'TuiButtonSelect',
    'TuiChip',
    'TuiDataListWrapper',
    'TuiFiles',
    'TuiFade',
    'TuiChevron',
    'TuiSelect',
    'TuiBlock',
    'TuiRadioList',
    'TuiPassword',
    'TuiSegmented',
    'TuiSwitch',
    'TuiTextarea',
    'TuiTooltip',
    'TuiConfirmService',
    'TuiProgress',
  ],
  'export const tuiInputFilesOptionsProvider = () => [];',
)
boundary(
  '@taiga-ui/cdk',
  ['TuiAnimated', 'TuiMapperPipe', 'TuiAutoFocus', 'TuiValidator'],
  'export const tuiMarkControlAsTouchedAndValidate = () => {}; export const tuiProvide = () => []; export const tuiIsString = value => typeof value === "string";',
)
boundary('@taiga-ui/layout', [
  'TuiCardLarge',
  'TuiHeader',
  'TuiForm',
  'TuiBlockStatus',
  'TuiNavigation',
])
boundary('./components/controls.component', ['MarketplaceControlsComponent'])
boundary('@taiga-ui/addon-table', [
  'TuiTable',
  'TuiTableDirective',
  'TuiTablePagination',
])
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
boundary('./package.component', ['SideloadPackageComponent'])
boundary(
  './sideload.utils',
  [],
  `
export const requests = [];
export const validateS9pk = file => new Promise(resolve => requests.push({ file, resolve }));
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
boundary('src/app/services/state.service', ['StateService'])
boundary('src/app/services/trusted-keys.service', ['TrustedKeysService'])
boundary('src/app/services/package-icon.service', ['PackageIconService'])
boundary('src/app/services/storage.service', ['StorageService'])
boundaries.set(
  '../storage.service',
  "export { StorageService } from 'src/app/services/storage.service';",
)
boundary('src/app/routes/portal/pipes/to-manifest', ['ToManifestPipe'])
boundary('../services/alerts.service', ['MarketplaceAlertsService'])
boundary('src/app/services/title.service', ['TitleDirective'])
boundary('src/app/routes/portal/components/form.component', ['FormComponent'])
boundary(
  './source-picker',
  [],
  'export const VOLUME_SOURCE_PICKER = {}; export const VOLUME_SOURCES = {};',
)
boundary('src/app/routes/portal/components/placeholder.component', [
  'PlaceholderComponent',
])
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
    'key-identity.ts',
    'types.ts',
    'registry-state.ts',
    'services/abstract-marketplace.service.ts',
    'util/icon.ts',
  ]
    .map(
      file =>
        `export * from ${JSON.stringify(new URL(`../../../../../shared-libs/ts-modules/marketplace/src/${file}`, import.meta.url).href)};`,
    )
    .join('\n') +
    '\nexport class RegistryConnectionStatus {} export class RegistryStatus {} export class StoreIconDirective {} export class MarketplaceComponent {} export class MarketplaceRegistrySelectComponent {} export class MarketplaceTileComponent {} export class MarketplacePackageLinkComponent {} export class MarketplaceSignersComponent {} export class MarketplaceSignerChipsComponent {} export class MarketplaceAboutComponent {} export class MarketplaceDependenciesComponent {} export class MarketplaceLinksComponent {} export class MarketplaceReleaseNotesComponent {}',
)
loadTypeScript(boundaries)
const { SwUpdate } = await import('@angular/service-worker')
const { ActivatedRoute, convertToParamMap, Router } =
  await import('@angular/router')
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
const {
  TaskService,
  CopyService,
  DialogService,
  ErrorService,
  i18nPipe,
  i18nService,
} = await import('@start9labs/shared')
const { TUI_BREAKPOINT } = await import('@taiga-ui/core')
const { CONTEXT } = await import('@taiga-ui/polymorpheus')
const { WA_WINDOW } = await import('@ng-web-apis/common')
const { UiUpdateService } = await import('src/app/services/ui-update.service')
const { ConfigService } = await import('src/app/services/config.service')
const { StateService } = await import('src/app/services/state.service')
const { TrustedKeysService } =
  await import('src/app/services/trusted-keys.service')
const { TrustedKeysService: ProductionTrustedKeysService } =
  await import('../src/app/services/trusted-keys.service.ts')
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
      publicKey: key,
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
  registryCache: {},
  osUpdateCache: null,
  serverInfo: {
    id: 'server-id',
    version,
    statusInfo: {},
    trustedKeys: trustedSigners,
    osRegistry: 'https://registry.test',
  },
  ui: { snakeHighScore: 0 },
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
  let osUpdateCache = null
  let revision = cachedVersion ? 1 : 0
  const requests = [],
    submissions = [],
    opened = [],
    alerts = [],
    tasks = [],
    confirmations = [],
    navigations = [],
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
              requests.push(params)
              assert.fail('Normal OS loads must not query a registry')
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
      provide: Router,
      useValue: { navigate: async path => navigations.push(path) },
    },
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
    navigations,
    updateErrors,
    availability,
    shown,
    get completed() {
      return completed
    },
    make: Class => runInInjectionContext(injector, () => new Class()),
    get: token => injector.get(token),
    live(version) {
      source.next([
        { id: ++revision, value: { ...state(version), osUpdateCache } },
      ])
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
    publish(
      versions = catalog,
      { registry = 'https://registry.test/', status = 'connected' } = {},
    ) {
      osUpdateCache = { registry, versions, status }
      source.next([
        {
          id: ++revision,
          patch: [
            { op: 'replace', path: '/osUpdateCache', value: osUpdateCache },
          ],
        },
      ])
    },
    selectRegistry(registry) {
      source.next([
        {
          id: ++revision,
          patch: [
            { op: 'replace', path: '/serverInfo/osRegistry', value: registry },
          ],
        },
      ])
    },
    async load(response = catalog) {
      this.publish(response)
      await os.loadOS()
    },
  }
}

function uiFixture(t, { enabled = true, controlled = true } = {}) {
  const serverState = new BehaviorSubject(null)
  let serverGitHash = runningGitHash
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
    { provide: ConfigService, useValue: { gitHash: runningGitHash } },
    { provide: StateService, useValue: serverState },
    { provide: WA_WINDOW, useValue: win },
  ])
  f.api.rpc.gitInfo = async () => `${serverGitHash}\n`
  const ui = f.get(UiUpdateService)
  return {
    ...f,
    serverState,
    async rebuild(state = 'running') {
      serverGitHash = nextGitHash
      serverState.next(state)
      await tick()
    },
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

test('a different server build tracks background installation without reloading the page', async t => {
  const f = uiFixture(t)
  assert.equal(f.ui.state().status, 'idle')
  assert.equal(f.checks.length, 0)
  await f.rebuild()
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
  test(`without an active worker only a build mismatch offers immediate reload: ${JSON.stringify(options)}`, async t => {
    const f = uiFixture(t, options)
    const component = f.make(RefreshAlertComponent)
    assert.equal(component.show(), false)
    await f.rebuild()
    assert.equal(f.ui.state().status, 'ready')
    assert.equal(f.checks.length, 0)
    assert.equal(component.show(), true)
    f.ui.refresh()
    assert.equal(f.reloads, 1)
    assert.deepEqual(f.tasks, [])
  })
}

test('a ready event received before version mismatch is retained and not undone by a later no-update event', async t => {
  const f = uiFixture(t)
  const component = f.make(RefreshAlertComponent)
  f.events.next(ready)
  assert.equal(component.show(), true)
  await f.rebuild()
  f.events.next(noUpdate)
  assert.equal(f.ui.state().status, 'ready')
  assert.equal(f.checks.length, 0)
})

test('an outdated page can refresh an already cached update even when the check downloads nothing new', async t => {
  const f = uiFixture(t)
  await f.rebuild()
  f.events.next(noUpdate)
  f.checks[0].resolve(false)
  await tick()
  assert.equal(f.ui.state().status, 'ready')
  f.ui.refresh()
  assert.equal(f.reloads, 1)
})

test('a false check without a no-new-version event is retryable, not falsely ready', async t => {
  const f = uiFixture(t)
  await f.rebuild()
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

test('a matching build is current whatever server version the database reports', async t => {
  const f = uiFixture(t)
  const component = f.make(RefreshAlertComponent)
  f.version('0.4.0.5')
  f.serverState.next('running')
  await tick()
  assert.equal(f.ui.outdated(), false)
  assert.equal(f.checks.length, 0)
  assert.equal(component.show(), false)
})

test('a different server build is announced only once the server is running', async t => {
  const f = uiFixture(t)
  await f.rebuild('initializing')
  assert.equal(f.ui.outdated(), false)
  assert.equal(f.checks.length, 0)
  f.serverState.next('running')
  await tick()
  assert.equal(f.ui.outdated(), true)
  assert.equal(f.checks.length, 1)
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
    await f.rebuild()
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

test('Refresh before sync completion never queues a reload', async t => {
  const f = uiFixture(t)
  f.ui.refresh()
  assert.equal(f.checks.length, 0)
  assert.equal(f.reloads, 0)
  await f.rebuild()
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
  await f.rebuild()
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
  await f.rebuild()
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
  await f.rebuild()
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
  await f.rebuild()
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

test('dismissing an update dialog does not hide the next state', async t => {
  const f = uiFixture(t)
  const component = f.make(RefreshAlertComponent)
  await f.rebuild()
  assert.equal(component.show(), true)
  component.dismissed.set(true)
  assert.equal(component.show(), false)
  f.events.next(ready)
  assert.equal(component.show(), true)
})

for (const liveBeforeResponse of [true, false]) {
  test(`cached older version is replaced by live current ${liveBeforeResponse ? 'before' : 'after'} cache publication`, async t => {
    const f = fixture(t, '0.4.0.1')
    const loading = f.os.loadOS()
    await tick()
    assert.deepEqual(f.requests, [])
    if (liveBeforeResponse) f.live('0.4.0.2')
    f.publish({ '0.4.0.2': { releaseNotes: 'installed' } })
    await loading
    if (!liveBeforeResponse) {
      assert.equal(f.os.updateCandidates()[0].version, '0.4.0.2')
      f.live('0.4.0.2')
    }
    assert.deepEqual(f.os.updateCandidates(), [])
    assert.equal(f.availability.at(-1), false)
    assert.equal(f.shown.at(-1), false)
    assert.equal(f.requests.length, 0)
  })
}

test('a slower load cannot replace the running hash or catalog after a registry switch', async t => {
  const f = fixture(t, '0.4.0.2')
  const firstHash = deferred()
  const secondHash = deferred()
  let requests = 0
  f.api.rpc.gitInfo = () =>
    ++requests === 1 ? firstHash.promise : secondHash.promise
  f.selectRegistry('https://first.test')
  const first = f.os.loadOS('https://first.test')
  f.selectRegistry('https://second.test')
  const second = f.os.loadOS('https://second.test')
  f.publish(
    {
      '0.4.0.2': {
        gitHash: runningGitHash,
        releaseNotes: 'installed',
        squashfs,
      },
      '0.4.0.3': { releaseNotes: 'second registry', squashfs },
    },
    { registry: 'https://second.test' },
  )
  secondHash.resolve(runningGitHash)
  await second
  firstHash.resolve('outdated-response')
  await first
  assert.deepEqual(f.os.updateCandidates(), [
    { version: '0.4.0.3', notes: 'second registry', squashfs },
  ])
  assert.deepEqual(f.requests, [])
})

test('a registry switch releases a load waiting for the previous source without publishing an error', async t => {
  const f = fixture(t, '0.4.0.2')
  f.selectRegistry('https://first.test')
  const first = f.os.loadOS('https://first.test')
  await tick()
  f.selectRegistry('https://second.test')
  await first
  assert.deepEqual(f.os.updateCandidates(), [])
  f.publish(
    { '0.4.0.3': { releaseNotes: 'selected', squashfs } },
    { registry: 'https://second.test' },
  )
  await f.os.loadOS('https://second.test')
  assert.equal(f.os.updateCandidates()[0].notes, 'selected')
})

test('without a cache, load waits for the first live state', async t => {
  const f = fixture(t)
  const loading = f.os.loadOS()
  await tick()
  assert.equal(f.requests.length, 0)
  assert.deepEqual(f.os.updateCandidates(), [])
  f.live('0.4.0.2')
  await tick()
  f.publish({ '0.4.0.2': { releaseNotes: 'installed' } })
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

test('strictly newer candidates sort descending; offline cache remains readable; version updates do not refetch', async t => {
  const f = fixture(t, '0.4.0.2')
  await f.load()
  assert.deepEqual(f.os.updateCandidates(), candidates)
  assert.equal(f.availability.at(-1), true)
  assert.equal(f.shown.at(-1), true)
  f.publish(catalog, { status: 'error' })
  assert.equal(await firstValueFrom(f.os.registryStatus$), 'error')
  await assert.rejects(f.os.loadOS(), /registry unavailable/)
  assert.deepEqual(f.os.updateCandidates(), candidates)
  f.version('0.4.0.3')
  assert.deepEqual(f.os.updateCandidates(), [candidates[0]])
  f.version('0.4.0.4')
  assert.deepEqual(f.os.updateCandidates(), [])
  assert.equal(f.requests.length, 0)
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
  f.publish({
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
  assert.equal(f.requests.length, 0)
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
  f.publish(catalog)
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
  f.publish(catalog)
  await Promise.all(f.tasks)
  await tick()
  assert.deepEqual(f.opened, [UPDATE])
  assert.deepEqual(f.alerts, [])
})

test('General does not announce up-to-date after a failed manual check', async t => {
  const f = fixture(t, '0.4.0.2')
  f.make(General).onUpdate()
  await tick()
  f.publish({}, { status: 'error' })
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
  assert.equal(f.requests.length, 0)
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
  assert.deepEqual(
    f.confirmations.map(c => c.options.label),
    ['OS update blocked'],
  )
  assert.deepEqual(f.alerts, [])
  assert.deepEqual(f.submissions, [])
  await Promise.all(f.tasks)

  f.trust({
    custom: {
      name: 'My custom release signer',
      publicKey: untrustedSigner,
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
  assert.deepEqual(
    f.confirmations.map(c => c.options.label),
    ['OS update blocked'],
  )
  assert.deepEqual(f.alerts, [])
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

test('individual keys match PEM, Der64 and legacy base64url signers in both trust contexts', () => {
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
        publicKey,
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
        [{ id: 'publisher', name: 'Publisher', publicKey }],
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
  assert.deepEqual(
    f.confirmations.map(c => c.options.label),
    ['OS update blocked'],
  )
  assert.deepEqual(f.alerts, [])
  assert.deepEqual(f.submissions, [])
  assert.equal(f.completed, 0)
  await Promise.all(f.tasks)
  assert.deepEqual(f.submissions, [])
  f.confirmations[0].response.next(true)
  assert.equal(f.completed, 1)
  assert.deepEqual(f.navigations, [['/system', 'trusted-keys']])
})

for (const assets of [{ x86_64: { signatures: {} } }, {}]) {
  test(`missing signatures or platform metadata blocks submission: ${JSON.stringify(assets)}`, async t => {
    const f = fixture(t, '0.4.0.2')
    await f.load({ '0.4.0.4': { releaseNotes: '', squashfs: assets } })
    await f.make(SystemUpdateModal).update()
    await tick()
    assert.deepEqual(
      f.confirmations.map(c => c.options.label),
      ['OS update blocked'],
    )
    assert.deepEqual(f.alerts, [])
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
  assert.deepEqual(
    f.confirmations.map(c => c.options.label),
    ['OS update blocked'],
  )
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
const { diskSource, cifsSource, usableDrives } =
  await import('src/app/routes/portal/routes/system/routes/volumes/create')
const { DriveVolumeForm } =
  await import('src/app/routes/portal/routes/system/routes/volumes/drive-form')
const { FormDialogService } =
  await import('src/app/services/form-dialog.service')
const { sourceEdit } =
  await import('src/app/routes/portal/routes/system/routes/volumes/source-fields')
const { AddSourceService } =
  await import('src/app/routes/portal/routes/system/routes/volumes/add')
const { default: Backups } =
  await import('src/app/routes/portal/routes/system/routes/backups/index')
const { default: BackupTargets } =
  await import('src/app/routes/portal/routes/system/routes/backups/targets')
const { BackupService } =
  await import('src/app/routes/portal/routes/system/routes/backups/backup.service')
const { BACKUP_PICKER } =
  await import('src/app/routes/portal/routes/system/routes/backups/picker')
const { DELETE_LOCAL } =
  await import('src/app/routes/portal/routes/system/routes/volumes/delete-local')
const { Mock: VolumeMock } = await import('src/app/services/api/api.fixures')
const { TuiNotificationService } = await import('@taiga-ui/core')
const { WA_IS_MOBILE } = await import('@ng-web-apis/platform')
const { getManifest, getCurrentManifest } =
  await import('src/app/utils/get-package-data')
const { getPartitionName } = await import('src/app/utils/get-partition-name')

for (const [label, result, expected] of [
  ['declined', of(false), 0],
  ['accepted', of(true), 1],
  ['dismissed', EMPTY, 0],
]) {
  test(`form dialog Cancel uses the existing ${label} discard guard`, async () => {
    const prompts = []
    let closed = 0
    const service = Object.assign(Object.create(FormDialogService.prototype), {
      i18n: { transform: value => value },
      formService: {
        withConfirm: options => {
          prompts.push(options)
          return result
        },
      },
    })
    await service.close({ complete: () => closed++ })
    assert.equal(closed, expected)
    assert.equal(prompts.length, 1)
    assert.equal(prompts[0].label, 'Unsaved changes')
  })
}

test('source editor preserves unsaved fields when an inspection replaces its entry', () => {
  const entry = signal({
    name: 'Music',
    source: {
      kind: 'cifs',
      hostname: 'nas',
      path: '/music',
      username: 'reader',
    },
    availability: 'unknown',
    available: null,
    error: null,
  })
  const edit = sourceEdit(entry)
  edit.name.set('My music')
  edit.hostname.set('new-nas')
  edit.path.set('/albums')
  edit.username.set('writer')
  edit.password.set('replacement')
  entry.update(value => ({
    ...value,
    availability: 'available',
    available: 4096,
    error: null,
  }))
  assert.equal(edit.name(), 'My music')
  assert.equal(edit.hostname(), 'new-nas')
  assert.equal(edit.path(), '/albums')
  assert.equal(edit.username(), 'writer')
  assert.equal(edit.password(), 'replacement')
  assert.equal(edit.changed(), true)
  assert.equal(edit.invalid(), false)
})

test('source editor opens legacy CIFS aliases clean and saves a real hostname edit once', async () => {
  const entry = signal({
    name: 'Original alias',
    source: {
      kind: 'cifs',
      hostname: 'nas',
      path: '/files',
      username: 'reader',
    },
    availability: 'available',
    available: 4096,
    error: null,
  })
  const edit = sourceEdit(entry)
  const calls = []
  const client = {
    rename: async params => calls.push(['rename', params]),
    updateCifs: async params => calls.push(['update', params]),
  }
  assert.equal(edit.changed(), false)
  entry.update(value => ({ ...value, available: 8192 }))
  assert.equal(edit.changed(), false)
  await edit.save(volumeA, client)
  assert.deepEqual(calls, [])
  edit.hostname.set('new-nas')
  assert.equal(edit.changed(), true)
  await edit.save(volumeA, client)
  assert.deepEqual(calls, [
    ['rename', { id: volumeA, name: 'new-nas' }],
    [
      'update',
      {
        id: volumeA,
        hostname: 'new-nas',
        path: '/files',
        username: 'reader',
        password: undefined,
      },
    ],
  ])
})

test('source editor follows an actual stored rename without treating it as an edit', () => {
  const entry = signal({
    name: 'Music',
    source: { kind: 'local' },
    availability: 'unknown',
    available: null,
    error: null,
  })
  const edit = sourceEdit(entry)
  edit.name.set('Unsaved')
  entry.update(value => ({ ...value, name: 'Renamed elsewhere' }))
  assert.equal(edit.name(), 'Renamed elsewhere')
  assert.equal(edit.changed(), false)
})

test('partition names distinguish partitions on one drive and fall back to their device path', () => {
  const drive = { vendor: 'Same vendor', model: 'Same drive' }
  assert.equal(
    getPartitionName({
      ...drive,
      label: 'Server One',
      logicalname: '/dev/sdb1',
    }),
    'Server One',
  )
  assert.equal(
    getPartitionName({
      ...drive,
      label: 'Server Two',
      logicalname: '/dev/sdb2',
    }),
    'Server Two',
  )
  for (const label of [null, '']) {
    assert.equal(
      getPartitionName({ label, logicalname: '/dev/sdb3' }),
      '/dev/sdb3',
    )
  }
})

const volumeA = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567'
const volumeB = 'BCDEFGHIJKLMNOPQRSTUVWXYZ234567A'

test('mock disk sources capture physical drive metadata independently of later inventory edits', t => {
  const disk = VolumeMock.Disks.find(disk => disk.logicalname === '/dev/sdc')
  const partition = disk.partitions[0]
  const original = structuredClone(disk)
  t.after(() => Object.assign(disk, original))
  const source = VolumeMock.diskSource('/dev/sdc1', '/media/photos')
  assert.deepEqual(source, {
    kind: 'disk',
    logicalname: VolumeMock.driveLink('/dev/sdc1'),
    path: 'media/photos',
    drive: {
      vendor: 'Samsung',
      model: 'T7',
      label: 'T7',
      capacity: 2000000000000,
    },
  })
  assert.deepEqual(
    VolumeMock.diskSource(source.logicalname).drive,
    source.drive,
  )
  assert.equal(VolumeMock.driveLink(source.logicalname), source.logicalname)
  disk.vendor = 'Replacement'
  partition.label = 'Changed'
  assert.equal(source.drive.vendor, 'Samsung')
  assert.equal(source.drive.label, 'T7')
})

function volumeFixture(t, packageData = {}, disks = VolumeMock.Disks) {
  const source = new Subject()
  const cache = new BehaviorSubject({
    id: 1,
    value: {
      packageData,
      disks,
      serverInfo: { id: 'server', statusInfo: { backupConversions: {} } },
      backupTargets: {
        localTarget: {
          name: 'On this server',
          source: { kind: 'local' },
          availability: 'available',
          available: 1000,
          error: null,
          checkedAt: new Date().toISOString(),
          startOs: {
            server: {
              hostname: 'server',
              timestamp: '2026-03-01T00:00:00Z',
              version: '0.4.2',
              formatStatus: 'current',
            },
          },
          legacyBackup: false,
        },
        nasTarget: {
          name: 'Office NAS',
          source: { kind: 'cifs', hostname: 'nas', path: '/backups' },
          availability: 'unavailable',
          available: null,
          error: 'Unreachable',
          checkedAt: null,
          startOs: {
            server: {
              hostname: 'server',
              timestamp: '2026-01-01T00:00:00Z',
              version: '0.4.2',
              formatStatus: 'current',
            },
            other: {
              hostname: 'other',
              timestamp: '2026-02-01T00:00:00Z',
              version: '0.4.2',
              formatStatus: 'interrupted',
            },
          },
          legacyBackup: false,
        },
      },
      sharedVolumes: {
        [volumeA]: {
          name: 'Photos',
          source: { kind: 'local' },
          grants: {},
          availability: 'available',
          available: 1234,
          error: null,
          checkedAt: new Date().toISOString(),
        },
        [volumeB]: {
          name: 'Music',
          source: { kind: 'cifs', hostname: 'nas.invalid', path: '/music' },
          grants: {},
          availability: 'unavailable',
          available: null,
          error: 'Old observation',
          checkedAt: new Date(Date.now() - 3600000).toISOString(),
        },
      },
    },
  })
  const patch = new PatchDB(source, cache)
  patch.start()
  const query = new BehaviorSubject({})
  let revision = 1
  const requests = [],
    results = [],
    errors = [],
    routes = [],
    grants = [],
    taskCalls = [],
    confirmations = [],
    components = [],
    decisions = [],
    formattedNotices = [],
    alerts = [],
    forms = [],
    formValues = []
  let completed = 0
  const api = {
    rpc: {
      disk: {
        format: async params => {
          requests.push({ kind: 'format', params })
          return `${params.disk}1`
        },
      },
      backup: {
        create: async params => {
          requests.push({ kind: 'backup', params })
        },
        target: {
          remove: async params =>
            requests.push({ kind: 'remove-target', params }),
          inspect: async params => {
            requests.push({ kind: 'inspect-target', params })
            return null
          },
        },
        history: {
          list: async params => {
            requests.push({ kind: 'history', params })
            return { runs: [], total: 0 }
          },
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
        remove: async params =>
          requests.push({ kind: 'remove-volume', params }),
        create: async params => {
          requests.push({ kind: 'create-volume', params })
          return volumeA
        },
        inspect: async () => ({
          availability: 'available',
          available: 0,
          error: null,
        }),
        setGrant: async params => {
          grants.push(params)
        },
      },
    },
  }
  const injector = createEnvironmentInjector([
    AddSourceService,
    BackupService,
    { provide: TUI_BREAKPOINT, useValue: () => 'desktopLarge' },
    { provide: OSService, useValue: { backingUp$: of(false) } },
    {
      provide: ActivatedRoute,
      useValue: {
        queryParamMap: query.pipe(
          map(params => ({ get: key => params[key] ?? null })),
        ),
      },
    },
    { provide: PatchDB, useValue: patch },
    { provide: ApiService, useValue: api },
    {
      provide: FormDialogService,
      useValue: {
        open(_, options) {
          forms.push(options)
          if (formValues.length)
            options.data.buttons[0].handler(formValues.shift())
        },
      },
    },
    {
      provide: DialogService,
      useValue: {
        openAlert(message, options) {
          alerts.push({ message, options })
          return EMPTY
        },
        openConfirm(options) {
          confirmations.push(options)
          return of(decisions.shift() ?? true)
        },
        openComponent(content, options) {
          components.push({ content, options })
          return of(decisions.shift() ?? true)
        },
      },
    },
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
    { provide: ConfigService, useValue: { version: '0.4.2' } },
    {
      provide: NonNullableFormBuilder,
      useValue: new FormBuilder().nonNullable,
    },
    { provide: WA_IS_MOBILE, useValue: false },
    { provide: i18nPipe, useValue: { transform: value => value } },
    {
      provide: TuiNotificationService,
      useValue: {
        open: message => {
          formattedNotices.push(message)
          return EMPTY
        },
      },
    },
    {
      provide: Router,
      useValue: {
        async navigate(route, extras) {
          routes.push(extras ? { route, extras } : route)
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
          create: params => api.rpc.volumes.create(params),
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
    confirmations,
    components,
    decisions,
    formattedNotices,
    alerts,
    forms,
    formValues,
    get completed() {
      return completed
    },
    query,
    attach(disks) {
      source.next([
        {
          id: ++revision,
          patch: [{ op: 'replace', path: '/disks', value: disks }],
        },
      ])
    },
    make: Class => runInInjectionContext(injector, () => new Class()),
  }
}

test('choosing an external drive opens its form dialog without a separate loading task', async t => {
  const f = volumeFixture(t)
  await f.make(AddSourceService).form('disk', async () => null)
  assert.equal(f.forms.length, 1)
  assert.deepEqual(f.taskCalls, [])
})

test('the external drive form creates a volume on an existing partition without erasing', async t => {
  const f = volumeFixture(t)
  const model = f.make(DriveVolumeForm)
  await tick()
  assert.ok(model.spec())
  await model.buttons[0].handler({
    name: 'Photos',
    drive: { selection: '/dev/sdc', value: { partition: '/dev/sdc1' } },
    path: 'library/photos',
  })
  assert.deepEqual(f.confirmations, [])
  assert.deepEqual(f.requests, [
    {
      kind: 'create-volume',
      params: {
        name: 'Photos',
        source: {
          kind: 'disk',
          logicalname: '/dev/sdc1',
          path: 'library/photos',
        },
      },
    },
  ])
})

test('erasing a drive confirms, formats it with the volume name, then creates the volume', async t => {
  const f = volumeFixture(t)
  const model = f.make(DriveVolumeForm)
  await tick()
  assert.equal(
    await model.buttons[0].handler({
      name: 'Media',
      drive: { selection: '/dev/sdb', value: { partition: 'format' } },
      path: null,
    }),
    true,
  )
  assert.equal(f.confirmations.length, 1)
  assert.deepEqual(f.requests, [
    { kind: 'format', params: { disk: '/dev/sdb', label: 'Media' } },
    {
      kind: 'create-volume',
      params: {
        name: 'Media',
        source: { kind: 'disk', logicalname: '/dev/sdb1', path: '' },
      },
    },
  ])
})

test('declining the erase confirmation leaves the drive untouched', async t => {
  const f = volumeFixture(t)
  const model = f.make(DriveVolumeForm)
  await tick()
  f.decisions.push(false)
  assert.equal(
    await model.buttons[0].handler({
      name: 'Media',
      drive: { selection: '/dev/sdb', value: { partition: 'format' } },
      path: null,
    }),
    false,
  )
  assert.deepEqual(f.requests, [])
})

test('a volume that fails after formatting reports that the drive was formatted', async t => {
  const f = volumeFixture(t)
  const model = f.make(DriveVolumeForm)
  await tick()
  f.api.rpc.volumes.create = async () => {
    throw { code: 0, message: 'create failed' }
  }
  assert.equal(
    await model.buttons[0].handler({
      name: 'Media',
      drive: { selection: '/dev/sdb', value: { partition: 'format' } },
      path: null,
    }),
    false,
  )
  assert.equal(
    f.errors[0].message,
    'The drive was formatted, but could not be added. create failed',
  )
})

test('the external drive form never offers the OS drive or a StartOS data drive', () => {
  const drive = (stablePath, extra = {}) => ({
    logicalname: stablePath,
    stablePath,
    partitions: [],
    guid: null,
    ...extra,
  })
  assert.deepEqual(
    usableDrives([
      drive('/dev/disk/by-path/usb-a'),
      drive('/dev/disk/by-path/nvme-a-part3'),
      drive('/dev/disk/by-path/usb-b', { guid: 'pool' }),
      drive('/dev/disk/by-path/usb-c', {
        partitions: [{ logicalname: '/dev/sdc1', guid: 'pool' }],
      }),
    ]).map(d => d.stablePath),
    ['/dev/disk/by-path/usb-a'],
  )
})

test('local and CIFS forms open their own dialogs', async t => {
  const f = volumeFixture(t)
  const model = f.make(AddSourceService)
  const create = params => f.api.rpc.volumes.create(params)
  f.formValues.push({ name: 'Shared' })
  await model.form('local', create)
  await model.form('cifs', create)
  await Promise.resolve()
  assert.equal(f.forms.length, 2)
  assert.deepEqual(
    f.requests.filter(r => r.kind === 'create-volume'),
    [
      {
        kind: 'create-volume',
        params: { name: 'Shared', source: { kind: 'local' } },
      },
    ],
  )
})

test('the external drive form follows the attached drives', async t => {
  const f = volumeFixture(t, {}, [])
  const model = f.make(DriveVolumeForm)
  await tick()
  assert.deepEqual(model.drives(), [])
  assert.equal(model.spec(), null)
  f.attach(VolumeMock.Disks)
  await tick()
  assert.equal(model.drives().length, VolumeMock.Disks.length)
  assert.ok(model.spec())
})

test('the external drive form marks partitions that contain a StartOS backup', async t => {
  const f = volumeFixture(t, {}, [])
  const model = f.make(DriveVolumeForm)
  f.attach(VolumeMock.Disks)
  await tick()
  const { values } = model.spec().drive.variants['/dev/sdc'].spec.partition
  assert.match(values['/dev/sdc1'], / · Contains a StartOS backup$/)
  assert.doesNotMatch(values['/dev/sdc2'], /StartOS backup/)
})

function editingDriveForm(t, partition = '/dev/sdc1') {
  const f = volumeFixture(t)
  const model = f.make(DriveVolumeForm)
  const builder = new FormBuilder()
  const form = builder.group({
    name: ['Edited name'],
    path: ['library/photos'],
    drive: builder.group({
      selection: ['/dev/sdc', Validators.required],
      value: builder.group({ partition: [partition, Validators.required] }),
    }),
  })
  model.form = () => ({ form })
  return { ...f, model, form }
}

test('a removed drive clears only the source choice even when reinsertion supersedes asynchronous specs', async t => {
  const f = editingDriveForm(t)
  await tick()
  f.attach([])
  f.attach(VolumeMock.Disks)
  await tick()
  assert.equal(f.form.get('drive.selection').value, null)
  assert.equal(f.form.valid, false)
  assert.equal(f.model.value.name, 'Edited name')
  assert.equal(f.model.value.path, 'library/photos')
  assert.equal(f.model.value.drive.selection, null)
  assert.ok(f.model.spec())
})

test('metadata updates retain valid drive choices and edits made while specs are rebuilding', async t => {
  const f = editingDriveForm(t)
  await tick()
  f.attach(structuredClone(VolumeMock.Disks))
  f.form.get('name').setValue('Edited during rebuild')
  f.form.get('path').setValue('new/path')
  await tick()
  assert.equal(f.model.value.name, 'Edited during rebuild')
  assert.equal(f.model.value.path, 'new/path')
  assert.equal(f.model.value.drive.selection, '/dev/sdc')
  assert.equal(f.model.value.drive.value.partition, '/dev/sdc1')
})

test('a missing selected partition invalidates the source without selecting erase-and-format', async t => {
  const f = editingDriveForm(t)
  await tick()
  f.attach(
    VolumeMock.Disks.map(disk =>
      disk.logicalname === '/dev/sdc' ? { ...disk, partitions: [] } : disk,
    ),
  )
  await tick()
  assert.equal(f.form.get('drive.selection').value, null)
  assert.equal(f.form.get('drive.value.partition').value, null)
  assert.equal(f.model.value.name, 'Edited name')
  assert.equal(f.model.value.path, 'library/photos')
})

test('a stale selected partition cannot be submitted after the drive disappears', async t => {
  const f = editingDriveForm(t)
  await tick()
  const selected = f.form.getRawValue()
  f.attach([])
  await tick()
  assert.equal(await f.model.buttons[0].handler(selected), false)
  assert.deepEqual(f.requests, [])
})

test('remove/reinsert while erase confirmation is open requires a fresh choice before formatting', async t => {
  const f = editingDriveForm(t, 'format')
  await tick()
  const confirmation = new Subject()
  f.model.dialogs.openConfirm = () => confirmation
  const saving = f.model.buttons[0].handler(f.form.getRawValue())
  f.attach([])
  f.attach(VolumeMock.Disks)
  confirmation.next(true)
  confirmation.complete()
  assert.equal(await saving, false)
  assert.deepEqual(f.requests, [])
  assert.equal(f.form.get('name').value, 'Edited name')
  assert.equal(f.form.get('path').value, 'library/photos')
})

test('erase confirmation rejects a different inventory description at the same device name', async t => {
  for (const replace of [
    disk => ({ ...disk, stablePath: `${disk.stablePath}-replacement` }),
    disk => ({ ...disk, vendor: `${disk.vendor}-replacement` }),
    disk => ({ ...disk, model: `${disk.model}-replacement` }),
    disk => ({ ...disk, capacity: disk.capacity + 1 }),
    disk => ({ ...disk, guid: `${disk.guid}-replacement` }),
    disk => ({
      ...disk,
      partitions: disk.partitions.map(partition => ({
        ...partition,
        guid: `${partition.guid}-replacement`,
      })),
    }),
    disk => ({ ...disk, partitions: [] }),
  ]) {
    const f = editingDriveForm(t, 'format')
    await tick()
    const confirmation = new Subject()
    f.model.dialogs.openConfirm = () => confirmation
    const saving = f.model.buttons[0].handler(f.form.getRawValue())
    f.attach(
      VolumeMock.Disks.map(disk =>
        disk.logicalname === '/dev/sdc' ? replace(disk) : disk,
      ),
    )
    await tick()
    confirmation.next(true)
    confirmation.complete()
    assert.equal(await saving, false)
    assert.deepEqual(f.requests, [])
  }
})

test('erase confirmation snapshots an inventory entry before an in-place cache update', async t => {
  const f = editingDriveForm(t, 'format')
  const drives = structuredClone(VolumeMock.Disks)
  f.attach(drives)
  await tick()
  const confirmation = new Subject()
  f.model.dialogs.openConfirm = () => confirmation
  const saving = f.model.buttons[0].handler(f.form.getRawValue())
  const disk = drives.find(disk => disk.logicalname === '/dev/sdc')
  assert.ok(disk)
  disk.model = `${disk.model}-replacement`
  f.attach(drives)
  await tick()
  confirmation.next(true)
  confirmation.complete()
  assert.equal(await saving, false)
  assert.deepEqual(f.requests, [])
})

test('erase confirmation accepts an unchanged inventory description', async t => {
  const f = editingDriveForm(t, 'format')
  await tick()
  const confirmation = new Subject()
  f.model.dialogs.openConfirm = () => confirmation
  const saving = f.model.buttons[0].handler(f.form.getRawValue())
  f.attach(structuredClone(VolumeMock.Disks))
  await tick()
  confirmation.next(true)
  confirmation.complete()
  assert.equal(await saving, true)
  assert.equal(
    f.requests.filter(request => request.kind === 'format').length,
    1,
  )
})

test('volume sources map blank optional fields to their wire defaults', () => {
  assert.deepEqual(diskSource('/dev/sdc1', null), {
    kind: 'disk',
    logicalname: '/dev/sdc1',
    path: '',
  })
  assert.deepEqual(
    cifsSource({
      name: 'A',
      hostname: 'nas',
      path: 'share',
      username: null,
      password: null,
    }),
    {
      kind: 'cifs',
      hostname: 'nas',
      path: 'share',
      username: '',
      password: null,
    },
  )
})

test('the volumes page re-checks only volumes whose last check is stale', async t => {
  const f = volumeFixture(t)
  const requested = []
  const error = new Error('inspect failed')
  f.api.rpc.volumes.inspect = async ({ id }) => {
    requested.push(id)
    throw error
  }
  f.make(SystemVolumes)
  await tick()
  assert.deepEqual(requested, [volumeB])
  assert.deepEqual(f.errors, [error])
  assert.deepEqual(f.taskCalls, [])
})

test('local volume deletion requires the typed-name dialog and external removal a plain confirm', async t => {
  const f = volumeFixture(t)
  const model = f.make(SystemVolumes)
  const local = { name: 'Local', source: { kind: 'local' } }
  f.decisions.push(false)
  await model.remove(volumeA, local)
  assert.deepEqual(f.requests, [])
  assert.deepEqual(f.taskCalls, [])
  assert.deepEqual(f.confirmations, [])
  assert.equal(f.components.at(-1).options.label, 'Delete permanently')
  assert.deepEqual(f.components.at(-1).options.data, {
    name: 'Local',
    warning:
      'Deleting this volume permanently erases its folder on this server and every file in it.',
  })
  for (const source of [
    { kind: 'disk', logicalname: '/dev/disk/by-uuid/photos', path: 'library' },
    { kind: 'cifs', hostname: 'nas', path: '/photos' },
  ]) {
    await model.remove(volumeB, { name: 'External', source })
    assert.equal(
      f.confirmations.at(-1).data.content,
      'Remove this volume and all service access? Data on the external drive or CIFS share will not be deleted.',
    )
    assert.equal(f.confirmations.at(-1).label, 'Remove')
    assert.equal(f.confirmations.at(-1).data.yes, 'Remove')
    assert.equal(f.taskCalls.at(-1), 'Removing')
    assert.deepEqual(f.requests.at(-1), {
      kind: 'remove-volume',
      params: { id: volumeB },
    })
  }
})

for (const [page, Page] of [
  ['backups', () => Backups],
  ['backup targets', () => BackupTargets],
]) {
  test(`the ${page} page re-checks only targets whose last check is stale`, async t => {
    const f = volumeFixture(t)
    f.make(Page())
    await tick()
    assert.deepEqual(
      f.requests.filter(r => r.kind === 'inspect-target'),
      [{ kind: 'inspect-target', params: { id: 'nasTarget' } }],
    )
  })
}

test('backup history loads its page and pagination writes only non-default query params', async t => {
  const f = volumeFixture(t)
  const model = f.make(Backups)
  assert.equal(model.history(), undefined)
  await tick()
  assert.deepEqual(model.history(), { runs: [], total: 0 })
  f.query.next({ page: '2', size: '25' })
  await tick()
  assert.deepEqual(
    f.requests.filter(r => r.kind === 'history').map(r => r.params),
    [
      { offset: 0, limit: 10 },
      { offset: 50, limit: 25 },
    ],
  )
  model.paginate({ page: 2, size: 25 })
  model.paginate({ page: 0, size: 10 })
  assert.deepEqual(
    f.routes.map(r => r.extras),
    [
      { queryParams: { page: 2, size: 25 }, queryParamsHandling: 'merge' },
      { queryParams: { page: null, size: null }, queryParamsHandling: 'merge' },
    ],
  )
})

test('backup history results distinguish success, item errors and failure', async t => {
  const f = volumeFixture(t)
  const model = f.make(Backups)
  const run = (server, item) => ({
    report: {
      server: { attempted: true, error: server },
      packages: { service: { error: null } },
      sharedVolumes: { [volumeA]: { error: item } },
    },
  })
  assert.equal(model.toResult(run(null, null)).label, 'Succeeded')
  assert.equal(
    model.toResult(run(null, 'disk full')).label,
    'Completed with errors',
  )
  assert.equal(model.toResult(run('target lost', null)).label, 'Failed')
})

test('local backup target deletion requires the typed-name dialog and external removal a plain confirm', async t => {
  const f = volumeFixture(t)
  const model = f.make(BackupTargets)
  const targets = f.make(BackupService).targets()
  f.decisions.push(false)
  await model.remove('localTarget', targets.localTarget)
  assert.deepEqual(f.confirmations, [])
  assert.equal(f.components.at(-1).content, DELETE_LOCAL)
  assert.deepEqual(f.components.at(-1).options.data, {
    name: 'On this server',
    warning:
      'Deleting this target permanently erases its folder on this server and every backup in it.',
  })
  assert.equal(f.requests.filter(r => r.kind === 'remove-target').length, 0)
  await model.remove('nasTarget', targets.nasTarget)
  assert.equal(f.confirmations.at(-1).label, 'Remove')
  assert.equal(
    f.confirmations.at(-1).data.content,
    'Remove this backup target? Backups on the external drive or CIFS share will not be deleted.',
  )
  assert.equal(f.taskCalls.at(-1), 'Removing')
  assert.deepEqual(f.requests.at(-1), {
    kind: 'remove-target',
    params: { id: 'nasTarget' },
  })
})

test('the restore picker lists every server backup newest first and disables unusable ones', async t => {
  const f = volumeFixture(t)
  const picker = f.make(BACKUP_PICKER.component)
  assert.deepEqual(
    picker
      .backups()
      .map(({ targetId, serverId, usable }) => [targetId, serverId, usable]),
    [
      ['localTarget', 'server', true],
      ['nasTarget', 'other', false],
      ['nasTarget', 'server', false],
    ],
  )
})

test('failed local deletion is reported and can be retried from the catalog', async t => {
  const f = volumeFixture(t)
  const model = f.make(SystemVolumes)
  const volume = model.volumes()[volumeA]
  const error = new Error('detach failed')
  f.api.rpc.volumes.remove = async () => {
    throw error
  }
  await model.remove(volumeA, volume)
  assert.equal(model.volumes()[volumeA], volume)
  assert.deepEqual(f.errors, [error])
  assert.deepEqual(f.taskCalls, ['Deleting'])
  f.api.rpc.volumes.remove = async params =>
    f.requests.push({ kind: 'remove-volume', params })
  await model.remove(volumeA, volume)
  assert.deepEqual(f.requests.at(-1), {
    kind: 'remove-volume',
    params: { id: volumeA },
  })
  assert.deepEqual(f.taskCalls, ['Deleting', 'Deleting'])
})

test('volume-only backup selection skips unavailable sources and sends an explicit empty service selection', async t => {
  const f = volumeFixture(t)
  const model = f.make(BackupsBackupComponent)
  assert.equal(model.hasSelection(), false)
  model.toggleGroup(model.volumes(), true)
  assert.equal(model.hasSelection(), true)
  assert.equal(model.items().find(item => item.id === volumeB).disabled, true)
  await model.createBackup('password')
  assert.deepEqual(f.requests[0].params.packageIds, [])
  assert.deepEqual(f.requests[0].params.sharedVolumeIds, [volumeA])
  assert.equal(f.completed, 1)
  model.toggleGroup(model.volumes(), false)
  assert.equal(model.hasSelection(), false)
})

test('group select-all omits installing services and unavailable volumes', t => {
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
  model.toggleGroup(model.volumes(), true)
  model.toggleGroup(model.packages(), true)
  assert.deepEqual(model.groupState(model.volumes()), true)
  assert.deepEqual(
    new Set(
      model
        .items()
        .filter(item => item.checked)
        .map(item => item.id),
    ),
    new Set(['installed', volumeA]),
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
  for (const [choice, expected] of [
    ['No Access', null],
    ['Read', { readonly: true }],
    ['Read/write', { readonly: false }],
  ]) {
    await model.save(choice)
    assert.deepEqual(f.grants.at(-1), {
      id: volumeA,
      packageId: 'service',
      grant: expected,
    })
    assert.equal(model.pending(), false)
    assert.equal(model.value(), choice)
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
  const saved = model.save('No Access')
  assert.equal(model.pending(), true)
  model.grant.set({ readonly: false })
  pending.resolve()
  await saved
  assert.equal(model.value(), 'Read/write')
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
  assert.equal(model.value(), 'No Access')
  await model.save('Read')
  model.volumeId.set(volumeB)
  assert.equal(model.value(), 'No Access')
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

test('production mocks support inspection, independent failed backups and new-local restore without a public password', async t => {
  const { MockApiService } =
    await import('src/app/services/api/embassy-mock-api.service')
  const { Mock } = await import('src/app/services/api/api.fixures')
  const { mockPatchData } = await import('src/app/services/api/mock-patch')
  const { WA_CRYPTO, WA_SESSION_STORAGE } = await import('@ng-web-apis/common')
  const { PATCH_CACHE } =
    await import('src/app/services/patch-db/patch-db-source')
  const originalInfo = structuredClone(Mock.BackupInfo)
  const originalNotifications = [...Mock.Notifications]
  const originalDisks = structuredClone(Mock.Disks)
  t.after(() => {
    Object.assign(Mock.BackupInfo, originalInfo)
    Mock.Notifications.splice(0, Infinity, ...originalNotifications)
    Mock.Disks.splice(0, Infinity, ...originalDisks)
  })
  const cache = new BehaviorSubject({
    id: 1,
    value: structuredClone(mockPatchData),
  })
  const injector = createEnvironmentInjector([
    { provide: WA_CRYPTO, useValue: webcrypto },
    { provide: WA_SESSION_STORAGE, useValue: {} },
    { provide: PATCH_CACHE, useValue: cache },
    { provide: ConfigService, useValue: { gitHash: runningGitHash } },
    { provide: i18nPipe, useValue: { transform: value => value } },
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
  assert.equal(await api.rpc.gitInfo({}), runningGitHash)
  assert.deepEqual(await api.rpc.backup.target.find({}), [])
  await api.rpc.backup.target.remove({ id: 'MOCKBACKUPDRIVE234567ABCDEFGHIJK' })
  const found = await api.rpc.backup.target.find({})
  assert.equal(found.length, 1)
  assert.match(found[0], /^[A-Z2-7]{32}$/)
  await api.rpc.backup.target.inspect({ id: found[0] })
  assert.equal(cache.value.value.backupTargets[found[0]].name, 'T7')
  assert.deepEqual(
    cache.value.value.backupTargets[found[0]].startOs,
    Mock.Disks[1].partitions[0].startOs,
  )
  assert.deepEqual(await api.rpc.backup.target.find({}), [])
  Mock.Disks[2].partitions[0].startOs = structuredClone(
    Mock.Disks[1].partitions[0].startOs,
  )
  const withoutLabel = await api.rpc.backup.target.find({})
  assert.equal(withoutLabel.length, 1)
  assert.equal(cache.value.value.backupTargets[withoutLabel[0]].name, 'SSK')
  assert.deepEqual(
    cache.value.value.backupTargets[withoutLabel[0]].source.drive,
    {
      vendor: 'SSK',
      model: null,
      label: null,
      capacity: 2000000000000,
    },
  )
  const targetId = await api.rpc.backup.target.create({
    name: 'Archive',
    source: { kind: 'local' },
  })
  assert.match(targetId, /^[A-Z2-7]{32}$/)
  assert.deepEqual(
    await api.rpc.backup.target.list({}),
    cache.value.value.backupTargets,
  )
  await assert.rejects(
    api.rpc.backup.target.cifs.remove({ id: targetId }),
    /CIFS target/,
  )
  const cifs = await api.rpc.backup.target.cifs.add({
    hostname: 'nas.local',
    path: '/archive',
    username: 'family',
    password: 'private-password',
  })
  const [cifsId] = Object.keys(cifs)
  assert.match(cifsId, /^[A-Z2-7]{32}$/)
  await api.rpc.backup.target.updateCifs({
    id: cifsId,
    hostname: 'new-nas.local',
    path: '/new-archive',
    username: 'family',
  })
  assert.deepEqual(cache.value.value.backupTargets[cifsId].source, {
    kind: 'cifs',
    hostname: 'new-nas.local',
    path: '/new-archive',
    username: 'family',
  })
  assert.doesNotMatch(
    JSON.stringify(cache.value.value.backupTargets),
    /private-password/,
  )
  await api.rpc.backup.target.cifs.remove({ id: cifsId })
  assert.equal(cache.value.value.backupTargets[cifsId], undefined)
  assert.deepEqual(
    api.mockSource({ kind: 'cifs', hostname: 'nas', path: 'share' }),
    {
      kind: 'cifs',
      hostname: 'nas',
      path: '/share',
      username: '',
    },
  )
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
  const diskSource = { kind: 'disk', logicalname: '/dev/sdz1' }
  const disk = await api.rpc.volumes.create({
    name: 'External',
    source: diskSource,
  })
  const stored = {
    kind: 'disk',
    logicalname: Mock.driveLink('/dev/sdz1'),
    drive: null,
  }
  assert.deepEqual(cache.value.value.sharedVolumes[disk].source, {
    ...stored,
    path: '',
  })
  const folder = await api.rpc.volumes.create({
    name: 'External folder',
    source: { ...diskSource, path: '/library/photos' },
  })
  assert.deepEqual(cache.value.value.sharedVolumes[folder].source, {
    ...stored,
    path: 'library/photos',
  })
  assert.match(local, /^[A-Z2-7]{32}$/)
  assert.match(remote, /^[A-Z2-7]{32}$/)
  assert.equal(
    cache.value.value.sharedVolumes[remote].availability,
    'unavailable',
  )
  assert.match(cache.value.value.sharedVolumes[remote].error, /mount error/)
  assert.doesNotMatch(
    JSON.stringify(cache.value.value.sharedVolumes),
    /private-password/,
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
  await api.mockRevision([
    {
      op: 'replace',
      path: `/backupTargets/${targetId}/legacyBackup`,
      value: true,
    },
  ])
  await api.rpc.backup.create({
    targetId,
    packageIds: [],
    sharedVolumeIds: [local, remote],
    password: 'password',
    oldPassword: null,
  })
  await api.rpc.backup.target.rename({ id: targetId, name: 'Later name' })
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
  const history = await api.rpc.backup.history.list({ offset: 0, limit: 1 })
  assert.equal(history.runs[0].targetId, targetId)
  assert.equal(history.runs[0].targetName, 'Archive')
  const report = history.runs[0].report
  assert.equal(report.sharedVolumes[local].error, null)
  assert.ok(report.sharedVolumes[remote].error)
  assert.deepEqual(report.packages, {})
  assert.equal(cache.value.value.serverInfo.statusInfo.backupProgress, null)
  assert.deepEqual(Mock.Notifications, originalNotifications)
  assert.equal(cache.value.value.backupTargets[targetId].name, 'Later name')
  const restored = await api.rpc.backup.volume.restore({
    targetId,
    serverId: '1234-5678-9876-5432',
    password: 'password',
    ids: [local],
  })
  assert.notEqual(restored.restored[local], local)
  assert.match(restored.restored[local], /^[A-Z2-7]{32}$/)
  const { checkedAt, ...copy } =
    cache.value.value.sharedVolumes[restored.restored[local]]
  assert.ok(checkedAt)
  assert.deepEqual(copy, {
    name: 'Renamed photos',
    source: { kind: 'local' },
    grants: {},
    availability: 'available',
    available: 812400000000,
    error: null,
  })
  await api.rpc.volumes.remove({ id: local })
  assert.equal(cache.value.value.sharedVolumes[local], undefined)
  await api.rpc.volumes.remove({ id: local })
  await api.rpc.volumes.remove({ id: disk })
  await api.rpc.volumes.remove({ id: remote })
  const formatted = await api.rpc.disk.format({ disk: '/dev/sdb' })
  assert.equal(
    cache.value.value.disks[0].partitions[0].label,
    'StartOS Backups',
  )
  assert.deepEqual(
    api.mockSource({ kind: 'disk', logicalname: Mock.driveLink(formatted) })
      .drive,
    {
      vendor: 'SanDisk',
      model: 'Extreme Pro',
      label: 'StartOS Backups',
      capacity: 2000000000000,
    },
  )
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

const {
  matchingTrustedKeys,
  packageSigners,
  packageSignerPolicy,
  SignerStatusesPipe,
  signingKeyIdentity,
  registrySigningKeys,
  keyIdentities,
} = await import('@start9labs/marketplace')
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

test('PEM and DER public keys share the complete SHA-256 SPKI fingerprint', () => {
  const der = 'MCowBQYDK2VwAyEAJdDg62o3oHIh4lXECn9gqORe+cCBbH/67MWjMxIMzDg='
  const expected = {
    name: null,
    emails: [],
    fingerprints: ['SHA256:N8LBiYUGYmV8l4H5YRPwqgC4n6y9uMYO9IIGemPc1Bs'],
  }
  assert.deepEqual(signingKeyIdentity(der), expected)
  assert.deepEqual(
    signingKeyIdentity(
      `-----BEGIN PUBLIC KEY-----\n${der}\n-----END PUBLIC KEY-----`,
    ),
    expected,
  )
})

function signerStatusFixture() {
  const first = 'MCowBQYDK2VwAyEAJdDg62o3oHIh4lXECn9gqORe+cCBbH/67MWjMxIMzDg='
  const second = 'MCowBQYDK2VwAyEAfeubYt/ozIfZspUZdmu2Fa6sK2psMnHVGxuVolo8u40='
  const unknown = 'MCowBQYDK2VwAyEAgkNzc04Bav9FbjK8hzF46OqjUL0bD9ZxuT7C1FgTDDE='
  const key = (name, publicKey, marketplace) => ({
    name,
    publicKey,
    marketplace,
    os: true,
    origin: 'manual',
    originRegistry: null,
    removable: true,
    gpgExports: [],
    contact: [],
  })
  return {
    first,
    second,
    unknown,
    signers: [unknown, second, first],
    keys: {
      [first]: key('First signer', first, true),
      [second]: key('Second signer', second, false),
    },
    pipe: new SignerStatusesPipe(),
  }
}

test('every package signer is listed: trusted, then untrusted, then unknown', () => {
  const { first, second, unknown, signers, keys, pipe } = signerStatusFixture()
  const shown = pipe.transform(signers, keys)
  assert.deepEqual(
    shown.map(signer => [signer.id, signer.status]),
    [
      [first, 'trusted'],
      [second, 'untrusted'],
      [unknown, 'unknown'],
    ],
  )
  assert.deepEqual(
    shown.map(signer => signer.name),
    ['First signer', 'Second signer', null],
  )
  assert.match(shown[2].fingerprints[0], /^SHA256:/)
})

test('package signer status ignores OS trust and repeated signatures in other encodings', () => {
  const { first, keys, pipe } = signerStatusFixture()
  keys[first].marketplace = false
  assert.deepEqual(
    pipe
      .transform(
        [
          first,
          `-----BEGIN PUBLIC KEY-----\n${first}\n-----END PUBLIC KEY-----`,
        ],
        keys,
      )
      .map(signer => [signer.id, signer.status]),
    [[first, 'untrusted']],
  )
})

test('no signers list nothing, and without a trust list every signer is unknown', () => {
  const { signers, keys, pipe } = signerStatusFixture()
  assert.deepEqual(pipe.transform([], keys), [])
  assert.deepEqual(
    pipe.transform(signers, {}).map(signer => signer.status),
    ['unknown', 'unknown', 'unknown'],
  )
})

test('known GPG signers show their label and every email address', () => {
  const { first, keys, pipe } = signerStatusFixture()
  keys[first].gpgExports = [
    {
      identities: {
        [first]: ['Alice <alice@example.test>', 'Alice <alice@other.test>'],
      },
      keyFingerprints: {
        [first]: ['5456dbff1b9df905041fa7765259adfc2d63c217'],
      },
    },
  ]
  assert.deepEqual(pipe.transform([first], keys), [
    {
      id: first,
      status: 'trusted',
      name: 'First signer',
      emails: ['alice@example.test', 'alice@other.test'],
      fingerprints: ['5456 DBFF 1B9D F905 041F A776 5259 ADFC 2D63 C217'],
    },
  ])
})

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
    publicKeyPem(Uint8Array.from([2, ...new Array(31).fill(0)])),
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

test('package signers select the install asset or claimed sideload keys, not other assets', () => {
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

test('individual keys match independently in each trust context and deduplicate repeated signatures', () => {
  const [first, second] = OS_UPDATE_SIGNERS
  const keys = {
    first: {
      name: 'Publisher',
      publicKey: first,
      marketplace: true,
      os: false,
    },
    second: {
      name: 'Publisher',
      publicKey: second,
      marketplace: true,
      os: false,
    },
  }
  assert.deepEqual(
    matchingTrustedKeys([first, first, second], keys, 'marketplace'),
    [
      { id: 'first', name: 'Publisher', publicKey: first },
      { id: 'second', name: 'Publisher', publicKey: second },
    ],
  )
  assert.deepEqual(matchingTrustedKeys([first, second], keys, 'os'), [])
  keys.first.os = true
  keys.first.marketplace = false
  assert.deepEqual(matchingTrustedKeys([first, second], keys, 'marketplace'), [
    { id: 'second', name: 'Publisher', publicKey: second },
  ])
  assert.deepEqual(matchingTrustedKeys([first, second], keys, 'os'), [
    { id: 'first', name: 'Publisher', publicKey: first },
  ])
})

test('package pins reuse the OS any-signer policy without granting OS-only or unknown keys', () => {
  const [first, second] = OS_UPDATE_SIGNERS
  const source = { s9pks: [], signers: [first, second, untrustedSigner] }
  const keys = {
    first: { name: 'Packages', publicKey: first, marketplace: true, os: false },
    second: {
      name: 'OS only',
      publicKey: second,
      marketplace: false,
      os: true,
    },
  }
  assert.deepEqual(packageSignerPolicy(source, keys, false), {
    any: [{ signer: first }],
  })
  assert.deepEqual(packageSignerPolicy(source, keys, true), {
    any: [{ signer: first }, { signer: second }, { signer: untrustedSigner }],
  })
  assert.deepEqual(packageSignerPolicy(source, {}, false), { any: [] })
  assert.deepEqual(packageSignerPolicy({ s9pks: [] }, keys, true), { any: [] })
})

function controlsFixture({
  sideload = false,
  trusted = false,
  confirmed = false,
} = {}) {
  const calls = [],
    prompts = []
  const instance = {
    file: () => (sideload ? new File(['data'], 'test.s9pk') : undefined),
    trusted: () => (trusted ? [{ name: 'Publisher' }] : []),
    localPkg: () => undefined,
    pkg: () => pkg,
    alerts: {
      alertPreDownload: async () => true,
      alertUntrusted: async () => {
        prompts.push('untrusted')
        return confirmed
      },
    },
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

test('an untrusted package installs only after confirmation, with the trust override', async () => {
  for (const [sideload, url] of [
    [true, null],
    [false, 'https://registry.test'],
  ]) {
    const declined = controlsFixture({ sideload })
    await declined.run()
    assert.deepEqual(declined.prompts, ['untrusted'])
    assert.deepEqual(declined.calls, [])
    const confirmed = controlsFixture({ sideload, confirmed: true })
    await confirmed.run()
    assert.deepEqual(confirmed.calls, [[url, true]])
  }
})

test('a trusted package installs without a prompt or override', async () => {
  for (const [sideload, url] of [
    [true, null],
    [false, 'https://registry.test'],
  ]) {
    const f = controlsFixture({ sideload, trusted: true })
    await f.run()
    assert.deepEqual(f.prompts, [])
    assert.deepEqual(f.calls, [[url, false]])
  }
})

test('the upload RPC receives the confirmed override and failed setup does not navigate', async () => {
  const calls = []
  const file = new File(['data'], 'test.s9pk')
  const signatures = { [untrustedSigner]: 'AA==' }
  const instance = {
    file: () => file,
    pkg: () => ({ s9pks: [], signers: [untrustedSigner], signatures }),
    trust: { keys: () => ({}) },
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
  assert.deepEqual(calls, [
    {
      trust: true,
      signatures,
      acceptSigners: { any: [{ signer: untrustedSigner }] },
    },
  ])
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

test('sideload pins only package-trusted keys and forwards the signature map', async () => {
  const calls = []
  const signatures = {
    [OS_UPDATE_SIGNERS[0]]: 'Ag==',
    [untrustedSigner]: 'AA==',
  }
  const instance = {
    file: () => new File(['data'], 'test.s9pk'),
    pkg: () => ({
      s9pks: [],
      signers: [OS_UPDATE_SIGNERS[0], untrustedSigner],
      signatures,
    }),
    trust: { keys: () => trustedSigners },
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
      uploadFile: async () => {},
    },
  }
  assert.equal(await Controls.prototype.upload.call(instance, false), true)
  assert.deepEqual(calls, [
    {
      trust: false,
      signatures,
      acceptSigners: { any: [{ signer: OS_UPDATE_SIGNERS[0] }] },
    },
  ])
})

test('marketplace installs send the matched signer pins with the explicit override intent', async () => {
  const submissions = []
  const instance = {
    patch: { watch$: () => of(trustedSigners) },
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
  for (const trust of [undefined, true]) {
    await MarketplaceService.prototype.installPackage.call(
      instance,
      pkg,
      'https://registry.test',
      trust,
    )
  }
  assert.deepEqual(submissions, [
    {
      id: pkg.id,
      version: pkg.version,
      registry: 'https://registry.test',
      trust: false,
      acceptSigners: { any: [{ signer: OS_UPDATE_SIGNERS[0] }] },
    },
    {
      id: pkg.id,
      version: pkg.version,
      registry: 'https://registry.test',
      trust: true,
      acceptSigners: { any: [{ signer: OS_UPDATE_SIGNERS[0] }] },
    },
  ])
})

const { AddTrustedKeyDialog } =
  await import('src/app/components/trusted-keys/form')
const { default: RegistriesPage } =
  await import('src/app/routes/portal/routes/system/routes/registries')

const runTask = async work => {
  try {
    await work()
    return true
  } catch {
    return false
  }
}

const { default: TrustedKeysPage } =
  await import('src/app/routes/portal/routes/system/routes/trusted-keys')
const { TRUSTED_KEY_DETAILS } =
  await import('src/app/components/trusted-keys/details')

for (const [context, expected] of [
  ['marketplace', { marketplace: false, os: false }],
  ['os', { marketplace: true, os: true }],
]) {
  for (const reject of [false, true]) {
    test(`table ${context} switch tracks the active request and clears it on ${reject ? 'failure' : 'success'}`, async () => {
      const response = deferred()
      const calls = []
      const key = { name: 'Publisher', marketplace: true, os: false }
      const trust = {
        saving: signal(false),
        tasks: { runInline: runTask },
        save: ProductionTrustedKeysService.prototype.save,
        update: ProductionTrustedKeysService.prototype.update,
        trust: ProductionTrustedKeysService.prototype.trust,
        api: {
          rpc: {
            server: {
              trustedKeys: {
                update: async params => {
                  calls.push(params)
                  await response.promise
                },
              },
            },
          },
        },
      }
      const page = { changing: signal(null), trust }
      const operation = TrustedKeysPage.prototype.toggle.call(
        page,
        'publisher',
        key,
        context,
      )
      assert.deepEqual(page.changing(), { id: 'publisher', context })
      assert.equal(trust.saving(), true)
      assert.deepEqual(calls, [
        { id: 'publisher', name: 'Publisher', ...expected },
      ])
      assert.deepEqual(key, { name: 'Publisher', marketplace: true, os: false })
      if (reject) response.reject(new Error('Trust change refused'))
      else response.resolve()
      await operation
      assert.equal(page.changing(), null)
      assert.equal(trust.saving(), false)
    })
  }
}

function keyDetailsFixture(t, key) {
  const id = 'MCowBQYDK2VwAyEAJdDg62o3oHIh4lXECn9gqORe+cCBbH/67MWjMxIMzDg='
  const response = deferred()
  const calls = []
  const injector = createEnvironmentInjector(
    [
      {
        provide: NonNullableFormBuilder,
        useValue: new FormBuilder().nonNullable,
      },
      { provide: CONTEXT, useValue: { data: id } },
      {
        provide: TrustedKeysService,
        useValue: {
          keys: signal(key ? { [id]: key } : {}),
          trust: async (...args) => {
            calls.push(['update', ...args])
            await response.promise
          },
          add: async (...args) => {
            calls.push(['add', ...args])
            await response.promise
          },
        },
      },
    ],
    null,
  )
  t.after(() => injector.destroy())
  return {
    id,
    calls,
    response,
    instance: runInInjectionContext(
      injector,
      () => new TRUSTED_KEY_DETAILS.component(),
    ),
  }
}

for (const [context, enabled] of [
  ['marketplace', false],
  ['os', true],
]) {
  test(`details ${context} switch changes only its selected permission`, async t => {
    const key = { name: 'Publisher', marketplace: true, os: false }
    const f = keyDetailsFixture(t, key)
    const operation = f.instance.toggle(context)
    assert.equal(f.instance.changing(), context)
    assert.deepEqual(f.calls, [['update', f.id, key, context, enabled]])
    f.response.resolve()
    await operation
    assert.equal(f.instance.changing(), null)
    assert.deepEqual(key, { name: 'Publisher', marketplace: true, os: false })
  })

  test(`unknown-key ${context} switch retains the atomic import flow`, async t => {
    const f = keyDetailsFixture(t)
    f.instance.form.controls.name.setValue('Publisher')
    const operation = f.instance.toggle(context)
    assert.equal(f.instance.changing(), context)
    assert.deepEqual(f.calls, [['add', f.id, 'Publisher', context]])
    f.response.resolve()
    await operation
    assert.equal(f.instance.changing(), null)
  })
}

test('unknown-key switches require a nonblank label before importing', async t => {
  const f = keyDetailsFixture(t)
  for (const name of ['', '  ']) {
    f.instance.form.controls.name.setValue(name)
    await f.instance.toggle('os')
    assert.deepEqual(f.calls, [])
    assert.equal(f.instance.changing(), null)
  }
})

test('unknown-key trust uses one atomic import request and clears saving on failure', async () => {
  for (const context of ['marketplace', 'os']) {
    for (const reject of [false, true]) {
      const calls = []
      const instance = {
        saving: signal(false),
        tasks: { runInline: runTask },
        save: ProductionTrustedKeysService.prototype.save,
        api: {
          rpc: {
            server: {
              trustedKeys: {
                add: async params => {
                  calls.push(['add', params])
                  if (reject) throw new Error('Import refused')
                  return { added: ['publisher'], existing: [] }
                },
                update: async params => calls.push(['update', params]),
              },
            },
          },
        },
      }
      await ProductionTrustedKeysService.prototype.add.call(
        instance,
        OS_UPDATE_SIGNERS[0],
        'Publisher',
        context,
      )
      assert.deepEqual(calls, [
        [
          'add',
          {
            name: 'Publisher',
            publicKeys: [OS_UPDATE_SIGNERS[0]],
            trustFor: context,
          },
        ],
      ])
      assert.equal(instance.saving(), false)
    }
  }
})

function addKeyFixture(t, reject = false) {
  const mutations = []
  const closed = []
  const result = { added: ['publisher'], existing: [] }
  const injector = createEnvironmentInjector(
    [
      {
        provide: NonNullableFormBuilder,
        useValue: new FormBuilder().nonNullable,
      },
      {
        provide: CONTEXT,
        useValue: {
          completeWith: value => closed.push(value),
          $implicit: { complete: () => closed.push(null) },
        },
      },
      {
        provide: ApiService,
        useValue: {
          rpc: {
            server: {
              trustedKeys: {
                add: async params => {
                  mutations.push(params)
                  if (reject) throw new Error('offline')
                  return result
                },
              },
            },
          },
        },
      },
      { provide: TaskService, useValue: { run: runTask } },
    ],
    null,
  )
  t.after(() => injector.destroy())
  return {
    instance: runInInjectionContext(injector, () => new AddTrustedKeyDialog()),
    mutations,
    closed,
    result,
  }
}

test('a manually added key submits its label and keys and closes with the server result', async t => {
  const f = addKeyFixture(t)
  f.instance.form.setValue({
    name: 'Publisher',
    publicKeys: OS_UPDATE_SIGNERS[0],
  })
  await f.instance.save()
  assert.deepEqual(f.mutations, [
    { name: 'Publisher', publicKeys: [OS_UPDATE_SIGNERS[0]] },
  ])
  assert.deepEqual(f.closed, [f.result])
  assert.equal(f.instance.saving(), false)
})

test('a manual key asks for a label only when it is PEM', async t => {
  const f = addKeyFixture(t)
  const gpg =
    '-----BEGIN PGP PUBLIC KEY BLOCK-----\n\n-----END PGP PUBLIC KEY BLOCK-----'
  assert.equal(f.instance.isPem(OS_UPDATE_SIGNERS[0]), true)
  assert.equal(f.instance.isPem(gpg), false)
  assert.deepEqual(f.instance.required({ value: ' ' }), { required: true })
  assert.equal(f.instance.required({ value: 'Publisher' }), null)
  f.instance.form.setValue({ name: 'Hidden', publicKeys: gpg })
  await f.instance.save()
  assert.deepEqual(f.mutations, [{ name: '', publicKeys: [gpg] }])
})

test('a manual key addition accepts exactly one key', async t => {
  const f = addKeyFixture(t)
  f.instance.form.setValue({
    name: 'Publisher',
    publicKeys: `${OS_UPDATE_SIGNERS[0]}\n${OS_UPDATE_SIGNERS[1]}`,
  })
  await f.instance.save()
  assert.deepEqual(f.mutations, [])
  assert.deepEqual(f.closed, [])
  assert.equal(f.instance.form.controls.publicKeys.hasError('publicKeys'), true)
})

test('a failed manual key addition keeps the dialog open', async t => {
  const f = addKeyFixture(t, true)
  f.instance.form.setValue({
    name: 'Publisher',
    publicKeys: OS_UPDATE_SIGNERS[0],
  })
  await f.instance.save()
  assert.deepEqual(f.closed, [])
  assert.equal(f.instance.saving(), false)
})

test('registry signing-key identities keep GPG metadata under the key label', () => {
  const id = 'MCowBQYDK2VwAyEAJdDg62o3oHIh4lXECn9gqORe+cCBbH/67MWjMxIMzDg='
  const gpgExports = [
    {
      armored: 'original export',
      fingerprints: ['5456dbff1b9df905041fa7765259adfc2d63c217'],
      userIds: ['Start9 <security@start9.com>'],
      identities: { [id]: ['Start9 <security@start9.com>'] },
      keyFingerprints: { [id]: ['5456dbff1b9df905041fa7765259adfc2d63c217'] },
    },
  ]
  const proposed = registrySigningKeys({
    trustSigners: {
      signer: { name: 'Label', contact: [], keys: [id], gpgExports },
    },
  })
  assert.deepEqual(Object.keys(proposed), [id])
  assert.equal(proposed[id].gpgExports, gpgExports)
  assert.deepEqual(signingKeyIdentity(id, proposed[id]), {
    name: 'Label',
    emails: ['security@start9.com'],
    fingerprints: ['5456 DBFF 1B9D F905 041F A776 5259 ADFC 2D63 C217'],
  })
})

test('key identities include GPG user IDs and registry contact emails', () => {
  const id = 'MCowBQYDK2VwAyEAJdDg62o3oHIh4lXECn9gqORe+cCBbH/67MWjMxIMzDg='
  const contact = [{ email: 'dev@example.com' }, { matrix: '@dev:example.com' }]
  assert.deepEqual(
    keyIdentities({
      publicKey: id,
      contact,
      gpgExports: [
        { identities: { [id]: ['A <a@example.com>', 'B <b@example.com>'] } },
        { identities: { [id]: ['A <a@example.com>'], other: ['C'] } },
      ],
    }),
    ['A <a@example.com>', 'B <b@example.com>', 'dev@example.com'],
  )
  assert.deepEqual(keyIdentities({ publicKey: id, contact, gpgExports: [] }), [
    'dev@example.com',
  ])
  assert.deepEqual(
    keyIdentities({
      publicKey: id,
      contact: [...contact, ...contact],
      gpgExports: [{ identities: { [id]: ['dev@example.com'] } }],
    }),
    ['dev@example.com'],
  )
  assert.deepEqual(
    keyIdentities({ publicKey: id, contact: [], gpgExports: [] }),
    [],
  )
})

function registriesFixture({
  publishes = true,
  confirm = true,
  orphaned = [],
  cleanup = true,
} = {}) {
  const calls = []
  const instance = {
    saving: signal(false),
    tasks: { run: runTask },
    os: {
      publishesUpdates: async url => {
        calls.push(['check', url])
        return publishes
      },
    },
    marketplace: {
      setOsRegistry: async url => {
        calls.push(['set', url])
        return ['key']
      },
      removeRegistry: async url => {
        calls.push(['remove', url])
        return orphaned
      },
    },
    api: {
      rpc: {
        server: {
          trustedKeys: {
            removeMany: async ({ ids }) => calls.push(['remove keys', ids]),
          },
        },
      },
    },
    dialogs: {
      openAlert: message => {
        calls.push(['alert', message])
        return of(null)
      },
      openConfirm: ({ label }) => {
        calls.push(['confirm', label])
        return of(confirm)
      },
      openComponent: (_, { label, data }) => {
        calls.push(['notice', label, data.ids])
        return of(cleanup)
      },
    },
    save: RegistriesPage.prototype.save,
    notify: RegistriesPage.prototype.notify,
  }
  return { instance, calls }
}

test('a registry that publishes no StartOS updates cannot provide them', async () => {
  const { instance, calls } = registriesFixture({ publishes: false })
  await RegistriesPage.prototype.useForUpdates.call(
    instance,
    'https://other.test',
  )
  assert.deepEqual(calls, [
    ['check', 'https://other.test'],
    ['alert', 'This registry does not publish StartOS updates.'],
  ])
})

test('another registry provides StartOS updates only after a warning, and reports the keys it trusted', async () => {
  for (const confirm of [false, true]) {
    const { instance, calls } = registriesFixture({ confirm })
    await RegistriesPage.prototype.useForUpdates.call(
      instance,
      'https://other.test',
    )
    assert.deepEqual(calls, [
      ['check', 'https://other.test'],
      ['confirm', 'Warning'],
      ...(confirm
        ? [
            ['set', 'https://other.test'],
            ['notice', 'StartOS updates', ['key']],
          ]
        : []),
    ])
  }
  const { instance, calls } = registriesFixture()
  await RegistriesPage.prototype.useForUpdates.call(
    instance,
    'https://registry.test',
  )
  assert.deepEqual(calls, [
    ['check', 'https://registry.test'],
    ['set', 'https://registry.test'],
    ['notice', 'StartOS updates', ['key']],
  ])
})

test('the update registry cannot be removed', async () => {
  const { instance, calls } = registriesFixture()
  await RegistriesPage.prototype.remove.call(instance, {
    url: 'https://registry.test',
    os: true,
  })
  assert.deepEqual(calls, [
    [
      'alert',
      'This registry provides StartOS updates. Choose another registry for StartOS updates before removing it.',
    ],
  ])
})

test('removing a registry offers to remove only the keys it left unoffered', async () => {
  for (const [orphaned, cleanup] of [
    [[], true],
    [['first', 'second'], false],
    [['first', 'second'], true],
  ]) {
    const { instance, calls } = registriesFixture({ orphaned, cleanup })
    await RegistriesPage.prototype.remove.call(instance, {
      url: 'https://other.test',
      os: false,
    })
    assert.deepEqual(calls, [
      ['confirm', 'Remove registry?'],
      ['remove', 'https://other.test'],
      ...(orphaned.length ? [['notice', 'Remove unused keys?', orphaned]] : []),
      ...(orphaned.length && cleanup ? [['remove keys', orphaned]] : []),
    ])
  }
})

test('failed registry-key cleanup submits one batch without per-key retries', async () => {
  const { instance, calls } = registriesFixture({
    orphaned: ['first', 'last-os'],
  })
  instance.api.rpc.server.trustedKeys.removeMany = async ({ ids }) => {
    calls.push(['remove keys', ids])
    throw new Error('Last OS key')
  }
  await RegistriesPage.prototype.remove.call(instance, {
    url: 'https://other.test',
    os: false,
  })
  assert.deepEqual(calls.at(-1), ['remove keys', ['first', 'last-os']])
  assert.equal(
    calls.filter(([operation]) => operation === 'remove keys').length,
    1,
  )
  assert.equal(instance.saving(), false)
})

test('connecting a registry asks for no approvals and resolves to the keys it trusted', async () => {
  const calls = []
  const instance = {
    api: {
      rpc: {
        server: {
          registries: {
            add: async params => {
              calls.push(params)
              return ['added']
            },
          },
        },
      },
    },
  }
  assert.deepEqual(
    await MarketplaceService.prototype.connectRegistry.call(
      instance,
      'https://registry.test',
    ),
    ['added'],
  )
  assert.deepEqual(calls, [{ url: 'https://registry.test', name: null }])
})

test('a late registry cache update cannot restore a removed registry', async t => {
  const { Exver } = await import('@start9labs/shared')
  const { StorageService } = await import('src/app/services/storage.service')
  const { PackageIconService } =
    await import('src/app/services/package-icon.service')
  const metadata = new BehaviorSubject({})
  const registries = new BehaviorSubject({
    'https://registry.test': 'Old name',
  })
  const writes = []
  const injector = createEnvironmentInjector(
    [
      {
        provide: PatchDB,
        useValue: {
          watch$: (...path) =>
            path[0] === 'registryCache' ? metadata : registries,
        },
      },
      { provide: Exver, useValue: {} },
      { provide: StorageService, useValue: { get: () => null } },
      { provide: PackageIconService, useValue: {} },
      { provide: ErrorService, useValue: { handleError: assert.fail } },
      {
        provide: ApiService,
        useValue: {
          rpc: {
            registry: {
              info: () =>
                assert.fail(
                  'Registry metadata must come from the shared cache',
                ),
              package: { get: async () => ({}) },
            },
            server: {
              registries: { remove: async () => registries.next({}) },
            },
            db: {
              put: {
                ui: async params => {
                  writes.push(params)
                  registries.next({ 'https://registry.test': params.value })
                },
              },
            },
          },
        },
      },
    ],
    null,
  )
  t.after(() => injector.destroy())
  const service = runInInjectionContext(
    injector,
    () => new MarketplaceService(),
  )
  const subscription = service.marketplace$.subscribe()
  t.after(() => subscription.unsubscribe())
  await service.delete('https://registry.test')
  metadata.next({
    'https://registry.test/': {
      status: 'connected',
      info: { name: 'New name' },
      packages: {},
    },
  })
  await new Promise(setImmediate)
  assert.deepEqual(await firstValueFrom(service.registries$), [])
  assert.deepEqual(writes, [])
})

test('registry removal uses the authoritative endpoint, clears a matching selected registry, and reports unoffered keys', async () => {
  const calls = []
  const instance = {
    api: {
      rpc: {
        server: {
          registries: {
            remove: async params => {
              calls.push(params)
              return ['orphaned']
            },
          },
        },
      },
    },
    storage: {
      get: () => 'https://registry.test',
      set: (...args) => calls.push(args),
    },
  }
  assert.deepEqual(
    await MarketplaceService.prototype.removeRegistry.call(
      instance,
      'https://registry.test',
    ),
    ['orphaned'],
  )
  assert.deepEqual(calls, [
    { url: 'https://registry.test' },
    ['selectedRegistry', null],
  ])
})

const { default: SideloadPage } =
  await import('src/app/routes/portal/routes/sideload/sideload.component')
const { requests: sideloadValidations } = await import('./sideload.utils')
const { SideloadPackageComponent } =
  await import('src/app/routes/portal/routes/sideload/package.component')

test('sideload License opens the complete archive text instead of its manifest identifier', async () => {
  let displayed
  SideloadPackageComponent.prototype.onStatic.call({
    pkg: () => ({
      license: 'MIT',
      fullLicense: 'Complete license fixture text',
    }),
    dialog: {
      openComponent: (_component, options) => {
        displayed = options.data
        return of(undefined)
      },
    },
  })
  assert.equal(await firstValueFrom(displayed), 'Complete license fixture text')
})

test('sideload validation cannot replace a newer file with stale details or errors', async () => {
  for (const stale of [{ ...pkg, title: 'First' }, 'Invalid package file']) {
    const page = new SideloadPage()
    const first = new File(['first'], 'first.s9pk')
    const second = new File(['second'], 'second.s9pk')
    const older = page.onFile(first)
    const newer = page.onFile(second)
    const firstValidation = sideloadValidations.shift()
    const secondValidation = sideloadValidations.shift()
    assert.equal(firstValidation.file, first)
    assert.equal(secondValidation.file, second)
    const selected = { ...pkg, title: 'Second' }
    secondValidation.resolve(selected)
    await newer
    firstValidation.resolve(stale)
    await older
    assert.equal(page.file, second)
    assert.equal(page.package(), selected)
    assert.equal(page.error(), null)
  }
})

test('clearing a pending sideload validation keeps its eventual result hidden', async () => {
  const page = new SideloadPage()
  const pending = page.onFile(new File(['first'], 'first.s9pk'))
  const validation = sideloadValidations.shift()
  page.clear()
  validation.resolve({ ...pkg, title: 'First' })
  await pending
  assert.equal(page.file, null)
  assert.equal(page.package(), null)
  assert.equal(page.error(), null)
})

test('OS first sync waits through Connecting and distinguishes empty success from failure', async t => {
  const f = fixture(t, '0.4.0.2')
  f.publish({}, { status: 'connecting' })
  let settled = false
  const loading = f.os.loadOS().then(() => {
    settled = true
  })
  await tick()
  assert.equal(settled, false)
  assert.equal(await firstValueFrom(f.os.registryStatus$), 'connecting')
  f.publish({})
  await loading
  assert.deepEqual(f.os.updateCandidates(), [])
  assert.equal(await firstValueFrom(f.os.registryStatus$), 'connected')
  f.publish(catalog, { status: 'connecting' })
  const retry = f.os.loadOS()
  await tick()
  assert.deepEqual(f.os.updateCandidates(), candidates)
  f.publish(catalog, { status: 'error' })
  await assert.rejects(retry, /registry unavailable/)
  assert.deepEqual(f.os.updateCandidates(), candidates)
})

test('switching the OS registry discards the old catalog until the matching cache arrives', async t => {
  const f = fixture(t, '0.4.0.2')
  await f.load()
  f.selectRegistry('https://other.test')
  assert.deepEqual(f.os.updateCandidates(), [])
  assert.equal(await firstValueFrom(f.os.registryStatus$), 'connecting')
  const loading = f.os.loadOS()
  f.publish(catalog)
  assert.deepEqual(f.os.updateCandidates(), [])
  f.publish(
    { '0.4.0.3': catalog['0.4.0.3'] },
    { registry: 'https://other.test/' },
  )
  await loading
  assert.deepEqual(f.os.updateCandidates(), [candidates[1]])
  assert.equal(await firstValueFrom(f.os.registryStatus$), 'connected')
  assert.deepEqual(f.requests, [])
})

test('a same-version candidate from another OS registry cannot replace an update during device lookup', async t => {
  const f = fixture(t, '0.4.0.2')
  await f.load()
  const modal = f.make(SystemUpdateModal)
  const device = deferred()
  f.api.rpc.server.deviceInfo = () => device.promise
  const updating = modal.update()
  await tick()
  f.selectRegistry('https://other.test')
  f.publish(catalog, { registry: 'https://other.test/' })
  device.resolve({ os: { platform: 'x86_64' } })
  await updating
  await Promise.all(f.tasks)
  assert.deepEqual(f.submissions, [])
})

const { Exver } = await import('@start9labs/shared')
const { StorageService } = await import('src/app/services/storage.service')
const { PackageIconService } =
  await import('src/app/services/package-icon.service')
const { PackageIconService: ProductionPackageIconService } =
  await import('../src/app/services/package-icon.service.ts')
const { default: MarketplacePage } =
  await import('../src/app/routes/portal/routes/marketplace/marketplace.component.ts')
const { RegistryRefresh } =
  await import('../src/app/routes/portal/components/registry-retry.ts')

const registryEntry = (version = '1.0.0:0', status = 'connected') => ({
  status,
  info: {
    name: 'Cached registry',
    categories: { bitcoin: { name: 'Bitcoin' } },
    icon: null,
  },
  packages: {
    example: {
      categories: ['bitcoin'],
      otherVersions: {},
      best: { [version]: { title: version } },
    },
  },
})

function marketplaceFixture(t, entries = {}) {
  const source = new Subject()
  const cache = new BehaviorSubject({
    id: 1,
    value: {
      ui: {},
      serverInfo: { registries: { 'https://registry.test': 'Saved name' } },
      registryCache: entries,
    },
  })
  const patch = new PatchDB(source, cache)
  patch.start()
  const queries = []
  const responses = []
  const refreshes = []
  const connections = []
  const iconRequests = []
  const iconResponses = []
  const storage = new Map()
  const routeParams = new BehaviorSubject(
    convertToParamMap({ registry: 'https://registry.test' }),
  )
  const injector = createEnvironmentInjector([
    { provide: PatchDB, useValue: patch },
    { provide: Exver, useValue: { getFlavor: () => null } },
    {
      provide: StorageService,
      useValue: {
        set: (key, value) => storage.set(key, value),
        get: key => storage.get(key),
      },
    },
    {
      provide: ConfigService,
      useValue: { defaultRegistry: 'https://registry.test' },
    },
    { provide: Router, useValue: { navigate: () => {} } },
    { provide: ActivatedRoute, useValue: { queryParamMap: routeParams } },
    {
      provide: TaskService,
      useValue: {
        run: async task => {
          await task()
          return true
        },
      },
    },
    {
      provide: PackageIconService,
      useFactory: () => new ProductionPackageIconService(),
    },
    { provide: MarketplaceService, useFactory: () => new MarketplaceService() },
    {
      provide: ApiService,
      useValue: {
        getStaticObjectUrl(url) {
          iconRequests.push(url)
          const response = deferred()
          iconResponses.push(response)
          return response.promise
        },
        rpc: {
          server: {
            registries: {
              refresh: async params => {
                refreshes.push(params)
              },
              add: async params => {
                connections.push(params)
                return []
              },
            },
          },
          registry: {
            info: () => assert.fail('Registry info must come from PatchDB'),
            package: {
              get: params => {
                queries.push(params)
                const response = deferred()
                responses.push(response)
                return response.promise
              },
            },
          },
        },
      },
    },
  ])
  const service = injector.get(MarketplaceService)
  let revision = 1
  const subscriptions = []
  t.after(() => {
    subscriptions.forEach(subscription => subscription.unsubscribe())
    injector.destroy()
    patch.stop()
  })
  return {
    service,
    queries,
    responses,
    refreshes,
    connections,
    iconRequests,
    iconResponses,
    storage,
    routeParams,
    makePage: () =>
      runInInjectionContext(injector, () => new MarketplacePage()),
    makeRefresh(url) {
      const refresh = runInInjectionContext(
        injector,
        () => new RegistryRefresh(),
      )
      refresh.registryRefresh = () => url
      return refresh
    },
    watch(observable) {
      const values = []
      subscriptions.push(observable.subscribe(value => values.push(value)))
      return values
    },
    replace(path, value) {
      source.next([{ id: ++revision, patch: [{ op: 'replace', path, value }] }])
    },
  }
}

test('registry availability distinguishes first sync, empty success and cached reconnect/error', async t => {
  const { isRegistryLoading } = await import('@start9labs/marketplace')
  const f = marketplaceFixture(t)
  f.makePage()
  const rows = f.watch(f.service.registries$)
  const errors = f.watch(f.service.requestErrors$)
  const stores = f.watch(f.service.currentRegistry$)
  const info = f.watch(f.service.fetchInfo$('https://registry.test/'))
  const packages = f.watch(f.service.getPackage$('missing', null, null))
  assert.equal(rows.at(-1)[0].status, 'connecting')
  assert.deepEqual(errors.at(-1), [])
  const entry = { ...registryEntry(), status: 'connecting', packages: {} }
  f.replace('/registryCache', { 'https://registry.test/': entry })
  assert.equal(
    isRegistryLoading(stores.at(-1).status, stores.at(-1).packages.length),
    true,
  )
  assert.deepEqual(info, [])
  assert.deepEqual(packages, [])
  f.replace('/registryCache', {
    'https://registry.test/': { ...entry, status: 'connected' },
  })
  assert.equal(
    isRegistryLoading(stores.at(-1).status, stores.at(-1).packages.length),
    false,
  )
  assert.equal(packages.at(-1), null)
  assert.equal(info.length, 1)
  for (const status of ['connected', 'connecting', 'error']) {
    f.replace('/registryCache', {
      'https://registry.test/': { ...registryEntry(), status },
    })
    assert.equal(stores.at(-1).packages.length, 1)
    assert.equal(stores.at(-1).status, status)
    assert.equal(
      isRegistryLoading(status, stores.at(-1).packages.length),
      false,
    )
    assert.deepEqual(
      errors.at(-1),
      status === 'error' ? ['https://registry.test/'] : [],
    )
  }
  f.replace('/registryCache', {
    'https://registry.test/': { ...entry, status: 'error' },
  })
  assert.equal(isRegistryLoading(stores.at(-1).status, 0), true)
  assert.equal(rows.at(-1)[0].status, 'error')
  f.replace('/registryCache', { 'https://registry.test/': entry })
  assert.equal(rows.at(-1)[0].status, 'connecting')
  assert.deepEqual(errors.at(-1), [])
  assert.deepEqual(f.queries, [])
})

test('registry failures project exact details to both the current catalog and management rows', t => {
  const f = marketplaceFixture(t)
  f.makePage()
  const rows = f.watch(f.service.registries$)
  const stores = f.watch(f.service.currentRegistry$)
  const message =
    'TLS certificate verification failed\n<img src=x onerror=alert(1)>'
  f.replace('/registryCache', {
    'https://registry.test/': {
      ...registryEntry(),
      status: 'error',
      error: message,
    },
  })
  assert.equal(rows.at(-1)[0].error, message)
  assert.equal(stores.at(-1).error, message)
  assert.equal(stores.at(-1).packages.length, 1)
  f.replace('/registryCache', {
    'https://registry.test/': { ...registryEntry(), status: 'connecting' },
  })
  assert.equal(rows.at(-1)[0].error, undefined)
  assert.equal(stores.at(-1).error, undefined)
  assert.equal(stores.at(-1).packages.length, 1)
  assert.deepEqual(f.queries, [])
})

test('malformed saved registries do not terminate valid registry listings or cache updates', async t => {
  const entry = registryEntry()
  const f = marketplaceFixture(t, { 'https://registry.test/': entry })
  const page = f.makePage()
  const saved = f.watch(f.service.registries$)
  const stores = f.watch(f.service.currentRegistry$)
  f.replace('/serverInfo/registries', {
    'https://user@invalid.test': 'Legacy URL',
    'ftp://invalid.test': 'Invalid scheme',
    'https://registry.test': 'Saved name',
  })
  await tick()
  assert.deepEqual(saved.at(-1), [
    {
      url: 'https://registry.test/',
      name: 'Saved name',
      status: 'connected',
      error: undefined,
    },
  ])
  assert.equal(page.isRegistrySaved(), true)
  assert.equal(stores.at(-1).packages.length, 1)

  f.replace('/serverInfo/registries', { 'https://registry.test': 'Renamed' })
  f.replace('/registryCache', {
    'https://registry.test/': registryEntry('2.0.0:0'),
  })
  await tick()
  assert.equal(saved.at(-1)[0].name, 'Renamed')
  assert.equal(stores.at(-1).packages[0].version, '2.0.0:0')
  assert.deepEqual(f.queries, [])
})

for (const status of ['connected', 'connecting', 'error']) {
  test(`manual refresh queues synchronization while ${status} without connecting or querying`, async t => {
    const url = 'https://registry.test/'
    const f = marketplaceFixture(t, { [url]: registryEntry('1.0.0:0', status) })
    await f.makeRefresh(url).refresh()
    assert.deepEqual(f.refreshes, [{ url }])
    assert.deepEqual(f.connections, [])
    assert.deepEqual(f.queries, [])
  })
}

test('the visible refresh action retries failed icons when the refreshed cache arrives and shares successful loads', async t => {
  const entry = registryEntry()
  const icon = 'blake3/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png'
  entry.packages.example.best['1.0.0:0'].icon = icon
  const f = marketplaceFixture(t, { 'https://registry.test/': entry })
  f.makePage()
  const refresh = f.makeRefresh('https://registry.test/')
  const visible = f.watch(
    f.service.currentRegistry$.pipe(
      switchMap(store =>
        f.service.fetchIcon$(store.url, store.packages[0].icon),
      ),
    ),
  )
  assert.equal(f.iconRequests.length, 1)
  f.iconResponses[0].reject(new Error('Icon temporarily unavailable'))
  await tick()
  assert.equal(visible.at(-1), 'fallback.png')
  await refresh.refresh()
  assert.deepEqual(f.refreshes, [{ url: 'https://registry.test/' }])
  assert.deepEqual(f.connections, [])
  assert.equal(f.iconRequests.length, 1)

  f.replace('/registryCache', {
    'https://registry.test/': { ...entry, status: 'error' },
  })
  assert.equal(f.iconRequests.length, 2)
  assert.equal(f.iconRequests[1], f.iconRequests[0])
  f.iconResponses[1].resolve('blob:recovered-icon')
  await tick()
  assert.equal(visible.at(-1), 'blob:recovered-icon')
  f.replace('/registryCache', {
    'https://registry.test/': { ...entry, status: 'connected' },
  })
  await tick()
  assert.equal(visible.at(-1), 'blob:recovered-icon')
  assert.equal(f.iconRequests.length, 2)
  assert.deepEqual(f.queries, [])
})

test('unsaved registry navigation requires explicit connection and malformed URLs do not end route updates', async t => {
  const f = marketplaceFixture(t)
  f.routeParams.next(
    convertToParamMap({ registry: 'https://unsaved.test/catalog' }),
  )
  const page = f.makePage()
  assert.equal(page.invalidRegistry(), false)
  assert.equal(page.isRegistrySaved(), false)
  assert.equal(page.selectedRegistry(), 'https://unsaved.test/catalog/')
  assert.deepEqual(f.connections, [])
  assert.deepEqual(f.queries, [])
  await page.connectRegistry()
  assert.deepEqual(f.connections, [
    { url: 'https://unsaved.test/catalog/', name: null },
  ])
  assert.equal(page.isRegistrySaved(), false)
  assert.equal(
    f.storage.get('selectedRegistry'),
    'https://unsaved.test/catalog/',
  )
  f.replace('/serverInfo/registries', {
    'https://unsaved.test/catalog/': 'Saved',
  })
  assert.equal(page.isRegistrySaved(), true)

  for (const registry of [
    'https://user:password@bad.test/',
    'ftp://bad.test',
    'https://bad.test/?query',
  ]) {
    f.routeParams.next(convertToParamMap({ registry }))
    assert.equal(page.invalidRegistry(), true)
  }
  f.routeParams.next(
    convertToParamMap({ registry: 'https://unsaved.test/catalog' }),
  )
  assert.equal(page.invalidRegistry(), false)
  assert.equal(page.isRegistrySaved(), true)
  assert.equal(f.connections.length, 1)
})

test('marketplace lists and metadata stream from canonical cache keys without navigation fetches', async t => {
  const f = marketplaceFixture(t)
  const stores = f.watch(f.service.marketplace$)
  const errors = f.watch(f.service.requestErrors$)
  const current = f.watch(f.service.currentRegistry$)
  await f.service.connect('https://registry.test')
  assert.deepEqual(stores.at(-1), { 'https://registry.test/': null })
  assert.deepEqual(errors.at(-1), [])
  assert.deepEqual(current, [])
  f.replace('/registryCache', { 'https://registry.test/': registryEntry() })
  assert.equal(
    stores.at(-1)['https://registry.test/'].packages[0].version,
    '1.0.0:0',
  )
  assert.equal(current.at(-1).info.name, 'Cached registry')
  assert.deepEqual(
    (await firstValueFrom(f.service.fetchInfo$('https://registry.test/')))
      .categories.all,
    { name: 'All' },
  )
  assert.equal(
    (await firstValueFrom(f.service.getPackage$('example', null, null)))
      .version,
    '1.0.0:0',
  )
  f.replace('/registryCache', {
    'https://registry.test/': registryEntry('2.0.0:0', 'error'),
  })
  assert.equal(current.at(-1).packages[0].version, '2.0.0:0')
  assert.deepEqual(errors.at(-1), ['https://registry.test/'])
  f.replace('/registryCache/https:~1~1registry.test~1/status', 'connected')
  assert.deepEqual(errors.at(-1), [])
  await f.service.connect('https://registry.test')
  f.replace('/serverInfo/registries', {})
  assert.deepEqual(stores.at(-1), {})
  assert.deepEqual(f.queries, [])
})

test('exact/source package queries wait for and track their registry cache, ignoring other registry changes and stale responses', async t => {
  const f = marketplaceFixture(t)
  const values = f.watch(
    f.service.fetchPackage$(
      'https://registry.test',
      'example',
      '1.0.0:0',
      null,
      '0.9.0:0',
    ),
  )
  assert.deepEqual(f.queries, [])
  const initial = registryEntry()
  f.replace('/registryCache', { 'https://registry.test/': initial })
  assert.deepEqual(f.queries, [
    {
      registry: 'https://registry.test',
      id: 'example',
      targetVersion: '=1.0.0:0',
      sourceVersion: '0.9.0:0',
      otherVersions: 'short',
    },
  ])
  f.replace('/registryCache', {
    'https://registry.test/': initial,
    'https://other.test/': registryEntry(),
  })
  assert.equal(f.queries.length, 1)
  f.replace(
    '/registryCache/https:~1~1registry.test~1',
    registryEntry('2.0.0:0', 'error'),
  )
  assert.equal(f.queries.length, 2)
  f.responses[1].resolve(registryEntry().packages.example)
  await tick()
  assert.equal(values.at(-1).version, '1.0.0:0')
  f.responses[0].resolve({ ...registryEntry().packages.example, best: {} })
  await tick()
  assert.equal(values.at(-1).version, '1.0.0:0')
})

test('browser bootstrap accepts snapshots saved before registry caches existed', async t => {
  const { LocalStorageBootstrap } =
    await import('src/app/services/patch-db/local-storage-bootstrap')
  const stored = {
    ui: { registries: {} },
    serverInfo: { version: '0.4.0.2' },
    packageData: {},
  }
  const injector = createEnvironmentInjector([
    { provide: StorageService, useValue: { get: () => stored } },
  ])
  t.after(() => injector.destroy())
  const bootstrap = runInInjectionContext(
    injector,
    () => new LocalStorageBootstrap(),
  )
  assert.deepEqual(bootstrap.init().value, {
    ...stored,
    registryCache: {},
    osUpdateCache: null,
  })
  assert.equal('registryCache' in stored, false)
})
