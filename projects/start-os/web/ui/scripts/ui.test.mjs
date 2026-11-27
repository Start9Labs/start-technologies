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
  defer,
  EMPTY,
  filter,
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
    'RpcError',
    'Exver',
    'ExverComparesPipe',
    'LeafProgressPipe',
  ],
  'export const LANGUAGES = []; export const LANGUAGE_TO_CODE = {}; export const getKeyboardName = () => ""; export const getAllKeyboardsSorted = () => []; export const pauseFor = async () => {}; export const FALLBACK_ICON = "fallback.png"; export const defaultRegistries = { start9: "https://registry.test", community: "https://community.test" }; export const isDefaultRegistry = url => Object.values(defaultRegistries).some(known => defaultUrl(known, url)); export const convertBytes = value => String(value); export const getErrorMessage = error => error.message; export const leafProgress = progress => progress; export const isNetworkError = error => error?.code === 0; ' +
    `export { sameUrl, registryUrl } from ${JSON.stringify(new URL('../../../../../shared-libs/ts-modules/shared/src/util/misc.util.ts', import.meta.url).href)};` +
    `import { sameUrl as defaultUrl } from ${JSON.stringify(new URL('../../../../../shared-libs/ts-modules/shared/src/util/misc.util.ts', import.meta.url).href)};` +
    `export { legacyDatabaseSubscription } from ${JSON.stringify(new URL('../../../../../shared-libs/ts-modules/shared/src/services/database-subscription.ts', import.meta.url).href)};` +
    `export { withAbortSignal } from ${JSON.stringify(new URL('../../../../../shared-libs/ts-modules/shared/src/util/with-abort.ts', import.meta.url).href)};` +
    `export { escapeHtml } from ${JSON.stringify(new URL('../../../../../shared-libs/ts-modules/shared/src/util/escape-html.ts', import.meta.url).href)};`,
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
    'TuiLabel',
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
    'TuiAccordion',
    'TuiPagination',
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
  'export const PATCH_CACHE = Symbol("patch cache"); export const PATCH_LIVE = Symbol("patch live");',
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
    'util/releases.ts',
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
const { PatchDB, SubtreePatchDB, projectRoot, selectorKey, watchSelection } =
  await import('patch-db-client')
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
const { ConnectionService } =
  await import('src/app/services/connection.service')
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
  { version: '0.4.0.4', notes: 'newest', squashfs, defaultLocale: 'pl_PL' },
  { version: '0.4.0.3', notes: 'newer', squashfs, defaultLocale: 'pl_PL' },
]
const state = version => ({
  registryCache: {},
  osUpdateCache: null,
  serverInfo: {
    id: 'server-id',
    version,
    gitHash: runningGitHash,
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
      gitInfo() {
        assert.fail('The running build hash comes from PatchDB')
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
    os.updateCandidates$.subscribe(value => availability.push(!!value.length)),
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
    restartFor(reason) {
      this.statusInfo('restart', reason)
    },
    statusInfo(field, value) {
      source.next([
        {
          id: ++revision,
          patch: [
            {
              op: 'replace',
              path: `/serverInfo/statusInfo/${field}`,
              value,
            },
          ],
        },
      ])
    },
    build(gitHash) {
      source.next([
        {
          id: ++revision,
          patch: [
            gitHash === undefined
              ? { op: 'remove', path: '/serverInfo/gitHash' }
              : { op: 'replace', path: '/serverInfo/gitHash', value: gitHash },
          ],
        },
      ])
    },
    publish(
      versions = catalog,
      { registry = 'https://registry.test/', status = 'connected' } = {},
    ) {
      osUpdateCache = { registry, versions, defaultLocale: 'pl_PL', status }
      source.next([
        {
          id: ++revision,
          patch: [
            { op: 'replace', path: '/osUpdateCache', value: osUpdateCache },
          ],
        },
      ])
    },
    publishRegistries(registryCache) {
      source.next([
        {
          id: ++revision,
          patch: [
            { op: 'replace', path: '/registryCache', value: registryCache },
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
      await os.checkForUpdates()
    },
  }
}

function uiFixture(t, { enabled = true, controlled = true } = {}) {
  const connection = new BehaviorSubject(false)
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
    { provide: ConnectionService, useValue: connection },
    { provide: WA_WINDOW, useValue: win },
  ])
  const ui = f.get(UiUpdateService)
  return {
    ...f,
    connection,
    async rebuild(live = true) {
      f.build(nextGitHash)
      connection.next(live)
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
  f.connection.next(true)
  await tick()
  assert.equal(f.ui.outdated(), false)
  assert.equal(f.checks.length, 0)
  assert.equal(component.show(), false)
})

test('a different server build is announced only once the database is live', async t => {
  const f = uiFixture(t)
  await f.rebuild(false)
  assert.equal(f.ui.outdated(), false)
  assert.equal(f.checks.length, 0)
  f.connection.next(true)
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

for (const stalled of ['checking', 'updating']) {
  test(`a sync the worker never settles offers Refresh after its deadline: ${stalled}`, async t => {
    t.mock.timers.enable({ apis: ['setTimeout'] })
    const f = uiFixture(t)
    await f.rebuild()
    if (stalled === 'updating') {
      t.mock.timers.tick(30_000)
      f.events.next(detected)
    }
    t.mock.timers.tick(59_999)
    assert.equal(f.ui.state().status, stalled)
    f.ui.refresh()
    assert.equal(f.reloads, 0)
    t.mock.timers.tick(1)
    assert.equal(f.ui.state().status, 'stalled')
    assert.equal(f.ui.phase(), 'stalled')
    assert.equal(f.ui.canRefresh(), true)
    assert.equal(f.reloads, 0)
    f.ui.refresh()
    assert.equal(f.reloads, 1)
  })
}

test('a worker report after the sync deadline still decides the outcome', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const f = uiFixture(t)
  await f.rebuild()
  t.mock.timers.tick(60_000)
  assert.equal(f.ui.state().status, 'stalled')
  f.events.next(failed)
  assert.equal(f.ui.state().status, 'error')
  f.ui.refresh()
  assert.equal(f.reloads, 0)
})

for (const found of [true, false]) {
  test(`a check completing after its deadline still decides the outcome: ${found}`, async t => {
    t.mock.timers.enable({ apis: ['setTimeout'] })
    const f = uiFixture(t)
    await f.rebuild()
    t.mock.timers.tick(60_000)
    f.checks[0].resolve(found)
    await tick()
    assert.equal(f.ui.state().status, found ? 'ready' : 'error')
    assert.equal(f.ui.canRefresh(), found)
    assert.equal(f.reloads, 0)
    f.ui.refresh()
    assert.equal(f.reloads, found ? 1 : 0)
  })
}

test('a cached-update report after the deadline wins over its false check result', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const f = uiFixture(t)
  await f.rebuild()
  t.mock.timers.tick(60_000)
  f.events.next(noUpdate)
  f.checks[0].resolve(false)
  await tick()
  assert.equal(f.ui.state().status, 'ready')
  assert.equal(f.ui.phase(), 'ready')
  assert.equal(f.reloads, 0)
})

test('an installation failure cancels the sync deadline', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const f = uiFixture(t)
  await f.rebuild()
  f.events.next(detected)
  f.events.next(failed)
  t.mock.timers.tick(60_000)
  assert.equal(f.ui.state().status, 'error')
  assert.equal(f.ui.canRefresh(), false)
  assert.equal(f.reloads, 0)
})

test('a stalled sync retains cache-reset recovery without claiming completion', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const f = uiFixture(t)
  await f.rebuild()
  t.mock.timers.tick(60_000)
  assert.equal(f.ui.phase(), 'stalled')
  assert.equal(f.ui.busy(), false)
  await f.ui.resetCache()
  assert.equal(f.ui.state().status, 'reloading')
  assert.equal(f.reloads, 1)
  assert.deepEqual(
    [...f.caches.keys()],
    ['ngsw:/other/:db:control', 'other-cache'],
  )
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

test('syncing remains dismissible; explicit navigation blocks dismissal', async t => {
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
    const loading = f.os.checkForUpdates()
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

test('a registry switch releases a load waiting for the previous source without publishing an error', async t => {
  const f = fixture(t, '0.4.0.2')
  f.selectRegistry('https://first.test')
  const first = f.os.checkForUpdates()
  await tick()
  f.selectRegistry('https://second.test')
  await first
  assert.deepEqual(f.os.updateCandidates(), [])
  f.publish(
    { '0.4.0.3': { releaseNotes: 'selected', squashfs } },
    { registry: 'https://second.test' },
  )
  await f.os.checkForUpdates()
  assert.deepEqual(f.os.updateCandidates(), [
    {
      version: '0.4.0.3',
      notes: 'selected',
      squashfs,
      defaultLocale: 'pl_PL',
    },
  ])
})

test('without a cache, load waits for the first live state', async t => {
  const f = fixture(t)
  const loading = f.os.checkForUpdates()
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
  await assert.rejects(f.os.checkForUpdates(), /registry unavailable/)
  assert.deepEqual(f.os.updateCandidates(), candidates)
  f.version('0.4.0.3')
  assert.deepEqual(f.os.updateCandidates(), [candidates[0]])
  f.version('0.4.0.4')
  assert.deepEqual(f.os.updateCandidates(), [])
  assert.equal(f.requests.length, 0)
  await f.load({ '0.4.0.5': { releaseNotes: 'refreshed', squashfs } })
  assert.deepEqual(f.os.updateCandidates(), [
    {
      version: '0.4.0.5',
      notes: 'refreshed',
      squashfs,
      defaultLocale: 'pl_PL',
    },
  ])
})

for (const publishes of [false, true]) {
  test(`OS publication waits for synchronization before resolving ${publishes}`, async t => {
    const f = fixture(t, '0.4.0.2')
    let settled = false
    const checking = f.os
      .publishesUpdates('https://registry.test')
      .then(value => {
        settled = true
        return value
      })
    await tick()
    assert.equal(settled, false)
    for (const entry of [
      { status: 'connected' },
      { status: 'connecting', publishesOsUpdates: false },
    ]) {
      f.publishRegistries({ 'https://registry.test/': entry })
      await tick()
      assert.equal(settled, false)
    }
    f.publishRegistries({
      'https://registry.test/': {
        status: 'connected',
        publishesOsUpdates: publishes,
      },
    })
    assert.equal(await checking, publishes)
    assert.deepEqual(f.requests, [])
  })
}

test('OS publication failure rejects instead of claiming the registry publishes no updates', async t => {
  const f = fixture(t, '0.4.0.2')
  const checking = assert.rejects(
    f.os.publishesUpdates('https://registry.test'),
    /connection timed out/,
  )
  f.publishRegistries({
    'https://registry.test/': {
      status: 'error',
      error: 'connection timed out',
      publishesOsUpdates: false,
    },
  })
  await checking
})

test('an observed OS publication remains usable while reconnecting or offline', async t => {
  const f = fixture(t, '0.4.0.2')
  for (const status of ['connecting', 'error']) {
    f.publishRegistries({
      'https://registry.test/': { status, publishesOsUpdates: true },
    })
    assert.equal(await f.os.publishesUpdates('https://registry.test'), true)
  }
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
    {
      version: '0.4.0.2',
      notes: 'next image',
      squashfs,
      defaultLocale: 'pl_PL',
    },
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

test('a downloaded update is no longer offered and blocks another update until restart', async t => {
  const f = fixture(t, '0.4.0.2')
  await f.load()
  assert.equal(f.shown.at(-1), true)
  assert.equal(await firstValueFrom(f.os.updateDisabled$), false)
  f.restartFor('update')
  assert.equal(f.availability.at(-1), true)
  assert.equal(f.shown.at(-1), false)
  assert.equal(await firstValueFrom(f.os.updateDisabled$), true)
})

for (const field of ['restarting', 'shuttingDown']) {
  test(`the restart reason is withheld while ${field}`, async t => {
    const f = fixture(t, '0.4.0.2')
    await f.load()
    const reasons = []
    const sub = f.os.restartReason$.subscribe(value => reasons.push(value))
    t.after(() => sub.unsubscribe())
    f.restartFor('update')
    assert.equal(reasons.at(-1), 'update')
    f.statusInfo(field, true)
    assert.equal(reasons.at(-1), null)
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

test('a same-version image is offered from the synced catalog without a manual check', async t => {
  const f = fixture(t, '0.4.0.2')
  f.publish({
    '0.4.0.2': { releaseNotes: 'next image', squashfs, gitHash: nextGitHash },
  })
  assert.deepEqual(f.os.updateCandidates(), [
    {
      version: '0.4.0.2',
      notes: 'next image',
      squashfs,
      defaultLocale: 'pl_PL',
    },
  ])
  assert.equal(f.availability.at(-1), true)
  f.build(nextGitHash)
  assert.deepEqual(f.os.updateCandidates(), [])
  assert.equal(f.availability.at(-1), false)
})

test('a database without the running build hash offers no same-version image', async t => {
  const f = fixture(t, '0.4.0.2')
  f.build(undefined)
  f.publish({
    ...catalog,
    '0.4.0.2': { releaseNotes: 'next image', squashfs, gitHash: nextGitHash },
  })
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
  const edit = sourceEdit(entry, { cifsNamedByHost: false })
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

test('source editor opens host-named CIFS aliases clean and saves a real hostname edit once', async () => {
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
  const edit = sourceEdit(entry, { cifsNamedByHost: true })
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

test('source editor keeps a CIFS volume name independent of its hostname', async () => {
  const entry = signal({
    name: 'Family files',
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
  const edit = sourceEdit(entry, { cifsNamedByHost: false })
  const calls = []
  const client = {
    rename: async params => calls.push(['rename', params]),
    updateCifs: async params => calls.push(['update', params]),
  }
  assert.equal(edit.hostNamed(), false)
  assert.equal(edit.name(), 'Family files')
  edit.hostname.set('new-nas')
  await edit.save(volumeA, client)
  assert.deepEqual(calls, [
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
  calls.length = 0
  edit.name.set('Photos')
  await edit.save(volumeA, client)
  assert.deepEqual(calls[0], ['rename', { id: volumeA, name: 'Photos' }])
})

test('source editor requires a CIFS hostname independently of its volume name', () => {
  const entry = signal({
    name: 'Family files',
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
  for (const cifsNamedByHost of [false, true]) {
    const edit = sourceEdit(entry, { cifsNamedByHost })
    assert.equal(edit.invalid(), false)
    for (const hostname of ['', '   ']) {
      edit.hostname.set(hostname)
      assert.equal(edit.invalid(), true)
    }
    edit.hostname.set('new-nas')
    assert.equal(edit.invalid(), false)
    edit.path.set('')
    assert.equal(edit.invalid(), true)
  }
})

test('source editor preserves literal volume names when editing or saving', async () => {
  for (const source of [
    { kind: 'local' },
    { kind: 'cifs', hostname: 'nas', path: '/files', username: 'reader' },
  ]) {
    const entry = signal({
      name: ' Photos ',
      source,
      availability: 'available',
      available: 4096,
      error: null,
    })
    const edit = sourceEdit(entry, { cifsNamedByHost: false })
    const calls = []
    const client = {
      rename: async params => calls.push(['rename', params]),
      updateCifs: async params => calls.push(['update', params]),
    }
    assert.equal(edit.changed(), false)
    assert.equal(edit.invalid(), false)
    await edit.save(volumeA, client)
    assert.deepEqual(calls, [])
    edit.name.set(' Photos  ')
    await edit.save(volumeA, client)
    assert.deepEqual(calls, [['rename', { id: volumeA, name: ' Photos  ' }]])
    edit.name.set('   ')
    assert.equal(edit.invalid(), true)
  }
})

test('source editor follows an actual stored rename without treating it as an edit', () => {
  const entry = signal({
    name: 'Music',
    source: { kind: 'local' },
    availability: 'unknown',
    available: null,
    error: null,
  })
  const edit = sourceEdit(entry, { cifsNamedByHost: false })
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
  await f
    .make(AddSourceService)
    .form('disk', { cifsNamedByHost: false }, async () => null)
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
  f.formValues.push(
    { name: 'Shared' },
    {
      name: 'Family files',
      hostname: 'nas',
      path: 'files',
      username: null,
      password: null,
    },
  )
  await model.form('local', { cifsNamedByHost: false }, create)
  await model.form('cifs', { cifsNamedByHost: false }, create)
  await Promise.resolve()
  assert.equal(f.forms.length, 2)
  assert.deepEqual(
    f.requests.filter(r => r.kind === 'create-volume').map(r => r.params.name),
    ['Shared', 'Family files'],
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

test('backup history filters preserve other route query parameters', async t => {
  const f = volumeFixture(t)
  const model = f.make(Backups)
  model.updateHistoryFilters({ historyKind: 'manual', historyPage: null })
  model.updateHistoryFilters({ historySearch: 'Photos', historyPage: null })
  assert.deepEqual(
    f.routes.map(r => r.extras),
    [
      {
        queryParams: { historyKind: 'manual', historyPage: null },
        queryParamsHandling: 'merge',
      },
      {
        queryParams: { historySearch: 'Photos', historyPage: null },
        queryParamsHandling: 'merge',
      },
    ],
  )
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

test('multiple volume restore errors are separate lines', async t => {
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
  assert.equal(f.errors[0], 'Music: second failure\nPhotos: first failure')
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
  for (const name of ['media/photos', '/', '.', '..', 'a\0b', '', '  ']) {
    await assert.rejects(
      api.rpc.volumes.create({ name, source: { kind: 'local' } }),
    )
    await assert.rejects(api.rpc.volumes.rename({ id: local, name }))
  }
  assert.equal(cache.value.value.sharedVolumes[local].name, 'Photos')
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
    name: 'Renamed photos (2)',
    source: { kind: 'local' },
    grants: {},
    availability: 'available',
    available: 812400000000,
    error: null,
  })
  await api.rpc.volumes.create({
    name: 'media-photos',
    source: { kind: 'local' },
  })
  Mock.BackupInfo.sharedVolumeBackups[remote].name = 'media/photos'
  const repaired = await api.rpc.backup.volume.restore({
    targetId,
    serverId: '1234-5678-9876-5432',
    password: 'password',
    ids: [remote],
  })
  assert.equal(
    cache.value.value.sharedVolumes[repaired.restored[remote]].name,
    'media-photos (2)',
  )
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
      { provide: ConnectionService, useValue: of(true) },
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
  await service.removeRegistry('https://registry.test')
  metadata.next({
    'https://registry.test/': {
      status: 'connected',
      info: { name: 'New name' },
      selections: {},
    },
  })
  await new Promise(setImmediate)
  assert.deepEqual(await firstValueFrom(service.registries$), [])
  assert.deepEqual(writes, [])
})

test('registry status keeps its last known observation while the server is disconnected', async t => {
  const { Exver } = await import('@start9labs/shared')
  const { StorageService } = await import('src/app/services/storage.service')
  const { PackageIconService } =
    await import('src/app/services/package-icon.service')
  const connected = new BehaviorSubject(true)
  const cache = new BehaviorSubject({
    'https://registry.test/': {
      status: 'error',
      error: 'connection timed out',
      info: { name: 'Test' },
      selections: {},
    },
  })
  const registries = of({ 'https://registry.test': 'Test' })
  const injector = createEnvironmentInjector(
    [
      {
        provide: PatchDB,
        useValue: {
          watch$: (...path) =>
            path[0] === 'registryCache' ? cache : registries,
        },
      },
      { provide: Exver, useValue: {} },
      { provide: StorageService, useValue: { get: () => null } },
      { provide: PackageIconService, useValue: {} },
      { provide: ApiService, useValue: {} },
      { provide: ConnectionService, useValue: connected },
      { provide: ErrorService, useValue: { handleError: assert.fail } },
    ],
    null,
  )
  t.after(() => injector.destroy())
  const service = runInInjectionContext(
    injector,
    () => new MarketplaceService(),
  )
  service.currentRegistryUrl$.next('https://registry.test/')
  const catalogs = []
  const subscription = service.marketplace$.subscribe(m => catalogs.push(m))
  t.after(() => subscription.unsubscribe())
  const current = async () => firstValueFrom(service.currentRegistry$)

  assert.deepEqual(await firstValueFrom(service.requestErrors$), [
    'https://registry.test/',
  ])
  assert.equal((await current()).status, 'error')
  const observed = catalogs.length

  connected.next(false)
  const [registry] = await firstValueFrom(service.registries$)
  assert.equal(registry.status, 'error')
  assert.equal(registry.error, 'connection timed out')
  assert.deepEqual(await firstValueFrom(service.requestErrors$), [
    'https://registry.test/',
  ])
  assert.equal((await current()).status, 'error')
  assert.equal(catalogs.length, observed)
  assert.equal(catalogs.at(-1)['https://registry.test/'].status, 'error')
})

test('header status separates offline, unreachable server, connecting, restarting and live', async t => {
  const { STATUS } = await import('src/app/services/status.service')
  const { NetworkService } = await import('src/app/services/network.service')
  const { PATCH_LIVE } =
    await import('src/app/services/patch-db/patch-db-source')
  const network = new BehaviorSubject(true)
  const reachable = new BehaviorSubject(null)
  const state = Object.assign(new BehaviorSubject('running'), {
    reachable$: reachable,
  })
  const live = new BehaviorSubject(false)
  const statusInfo = new BehaviorSubject({
    restarting: false,
    shuttingDown: false,
  })
  const { ɵINJECTOR_SCOPE } = await import('@angular/core')
  const injector = createEnvironmentInjector(
    [
      { provide: ɵINJECTOR_SCOPE, useValue: 'root' },
      { provide: NetworkService, useValue: network },
      { provide: StateService, useValue: state },
      { provide: PATCH_LIVE, useValue: live },
      { provide: PatchDB, useValue: { watch$: () => statusInfo } },
    ],
    null,
  )
  t.after(() => injector.destroy())
  const status = injector.get(STATUS)
  const connected = []
  const subscription = injector
    .get(ConnectionService)
    .subscribe(value => connected.push(value))
  t.after(() => subscription.unsubscribe())

  assert.equal(status().message, 'Connecting')
  reachable.next(false)
  assert.equal(status().message, "Can't reach server")
  reachable.next(true)
  assert.equal(status().message, 'Connecting')
  live.next(true)
  assert.equal(status().message, 'Connected')
  live.next(false)
  assert.equal(state.value, 'running')
  assert.equal(status().message, 'Connecting')
  reachable.next(false)
  assert.equal(status().message, "Can't reach server")
  statusInfo.next({ restarting: true, shuttingDown: false })
  assert.equal(status().message, 'Restarting')
  statusInfo.next({ restarting: false, shuttingDown: true })
  assert.equal(status().message, 'Shutting down')
  network.next(false)
  assert.equal(status().message, 'No Internet')
  assert.deepEqual(connected, [false, true, false])
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
  const loading = f.os.checkForUpdates().then(() => {
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
  const retry = f.os.checkForUpdates()
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
  const loading = f.os.checkForUpdates()
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
  selections: {
    example: {
      best: [version],
      versions: { [version]: { s9pks: [0], compatible: true } },
    },
  },
})

const registryIndex = (versions = ['1.0.0:0', '2.0.0:0', '3.0.0:0']) => ({
  package: {
    packages: {
      example: {
        authorized: {},
        categories: ['bitcoin'],
        versions: Object.fromEntries(
          versions.map(version => [
            version,
            { title: version, description: { short: '', long: '' }, s9pks: [] },
          ]),
        ),
      },
    },
    categories: {},
  },
})

const indexedRelease = version =>
  `/registryIndexes/https:~1~1registry.test~1/package/packages/example/versions/${version}`

function marketplaceFixture(t, entries = {}, index = registryIndex()) {
  const source = new Subject()
  const connection = new BehaviorSubject(true)
  const cache = new BehaviorSubject({
    id: 1,
    value: {
      ui: {},
      serverInfo: { registries: { 'https://registry.test': 'Saved name' } },
      registryCache: entries,
      registryIndexes: { 'https://registry.test/': index },
    },
  })
  const patch = new PatchDB(source, cache)
  patch.watchProjection$ = (projection, ...path) =>
    patch
      .watch$(...path)
      .pipe(map(value => projectRoot(watchSelection(projection), value)))
  patch.start()
  const queries = []
  const responses = []
  const errors = []
  const refreshes = []
  const connections = []
  const iconRequests = []
  const iconResponses = []
  const storage = new Map()
  const routes = []
  const routeParams = new BehaviorSubject(
    convertToParamMap({ registry: 'https://registry.test' }),
  )
  const injector = createEnvironmentInjector([
    { provide: PatchDB, useValue: patch },
    {
      provide: Exver,
      useValue: {
        getFlavor: () => null,
        compareExver: (a, b) =>
          Math.sign(a.localeCompare(b, undefined, { numeric: true })),
      },
    },
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
    {
      provide: Router,
      useValue: { navigate: (_, extras) => routes.push(extras?.queryParams) },
    },
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
    {
      provide: ErrorService,
      useValue: { handleError: error => errors.push(error) },
    },
    { provide: ConnectionService, useValue: connection },
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
    patch,
    injector,
    source,
    cache,
    connection,
    queries,
    responses,
    errors,
    refreshes,
    connections,
    iconRequests,
    iconResponses,
    storage,
    routes,
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
  const entry = { ...registryEntry(), status: 'connecting', selections: {} }
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

test('saved registries list the defaults first, in their declared order', t => {
  const f = marketplaceFixture(t)
  const rows = f.watch(f.service.registries$)
  f.replace('/serverInfo/registries', {
    'https://alpha.test/': 'Alpha',
    'https://community.test/': 'Community',
    'https://custom.test/': 'Custom',
    'https://registry.test/': 'Start9',
  })
  assert.deepEqual(
    rows.at(-1).map(row => row.name),
    ['Start9', 'Community', 'Alpha', 'Custom'],
  )
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
  const index = registryIndex()
  const icon = 'blake3/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.png'
  index.package.packages.example.versions['1.0.0:0'].icon = icon
  const f = marketplaceFixture(t, { 'https://registry.test/': entry }, index)
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

test('a remembered registry the server no longer saves falls back to the default', async t => {
  const f = marketplaceFixture(t)
  f.storage.set('selectedRegistry', 'https://removed.test/')
  f.routeParams.next(convertToParamMap({}))
  f.makePage()
  await tick()
  assert.deepEqual(f.routes.at(-1), { registry: 'https://registry.test' })
  assert.equal(f.storage.get('selectedRegistry'), null)

  f.storage.set('selectedRegistry', 'https://registry.test/')
  f.routeParams.next(convertToParamMap({}))
  await tick()
  assert.deepEqual(f.routes.at(-1), { registry: 'https://registry.test/' })
  assert.equal(f.storage.get('selectedRegistry'), 'https://registry.test/')
})

test('a pending remembered-registry fallback yields to an explicit route selection', async t => {
  const f = marketplaceFixture(t)
  const dump = f.cache.value
  f.cache.next({ id: 0, value: {} })
  f.storage.set('selectedRegistry', 'https://removed.test/')
  f.routeParams.next(convertToParamMap({}))
  const page = f.makePage()
  assert.deepEqual(f.routes, [])

  f.routeParams.next(convertToParamMap({ registry: 'https://explicit.test/' }))
  f.source.next([dump])
  await tick()
  assert.equal(page.selectedRegistry(), 'https://explicit.test/')
  assert.deepEqual(f.routes, [])
  assert.equal(f.storage.get('selectedRegistry'), 'https://removed.test/')
})

test('destroying the marketplace page cancels its pending remembered-registry fallback', async t => {
  const f = marketplaceFixture(t)
  const dump = f.cache.value
  f.cache.next({ id: 0, value: {} })
  f.storage.set('selectedRegistry', 'https://removed.test/')
  f.routeParams.next(convertToParamMap({}))
  const pageInjector = createEnvironmentInjector([], f.injector)
  t.after(() => {
    if (!pageInjector.destroyed) pageInjector.destroy()
  })
  runInInjectionContext(pageInjector, () => new MarketplacePage())
  pageInjector.destroy()

  f.source.next([dump])
  await tick()
  assert.deepEqual(f.routes, [])
  assert.equal(f.storage.get('selectedRegistry'), 'https://removed.test/')
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

test('a connected catalog shows no listing until its index projection arrives', async t => {
  const f = marketplaceFixture(t, { 'https://registry.test/': registryEntry() })
  const projected = new Subject()
  f.patch.watchProjection$ = () => projected
  const marketplace = f.watch(f.service.marketplace$)
  await f.service.connect('https://registry.test')
  const stores = f.watch(f.service.currentRegistry$)
  assert.deepEqual(stores, [])
  assert.ok(marketplace.length > 0)
  assert.ok(marketplace.every(m => m['https://registry.test/'] === null))

  projected.next(registryIndex().package.packages)
  assert.deepEqual(
    stores.map(store => store.packages.map(({ version }) => version)),
    [['1.0.0:0']],
  )
  assert.deepEqual(
    marketplace
      .at(-1)
      ['https://registry.test/'].packages.map(({ version }) => version),
    ['1.0.0:0'],
  )
})

test('a status-only registry change updates the listing without resubscribing the index projection', async t => {
  const f = marketplaceFixture(t, { 'https://registry.test/': registryEntry() })
  const watchProjection$ = f.patch.watchProjection$
  let subscriptions = 0
  f.patch.watchProjection$ = (...args) =>
    defer(() => {
      subscriptions++
      return watchProjection$(...args)
    })
  await f.service.connect('https://registry.test')
  const stores = f.watch(f.service.currentRegistry$)
  assert.equal(stores.at(-1).status, 'connected')
  assert.equal(subscriptions, 1)

  f.replace('/registryCache', {
    'https://registry.test/': {
      ...registryEntry('1.0.0:0', 'error'),
      error: 'connection timed out',
    },
  })
  assert.equal(stores.at(-1).status, 'error')
  assert.equal(stores.at(-1).error, 'connection timed out')
  assert.ok(stores.every(store => store.packages.length === 1))
  assert.equal(subscriptions, 1)
})

test('marketplace tolerates browser snapshots without selections and consumes the live replacement', async t => {
  const legacy = {
    status: 'connected',
    info: registryEntry().info,
    packages: {
      example: {
        categories: ['bitcoin'],
        best: { '1.0.0:0': { title: 'Legacy' } },
        otherVersions: {},
      },
    },
  }
  const f = marketplaceFixture(t, { 'https://registry.test/': legacy })
  const marketplace = f.watch(f.service.marketplace$)
  await f.service.connect('https://registry.test')
  const stores = f.watch(f.service.currentRegistry$)
  const packages = f.watch(f.service.getPackage$('example', null, null))
  assert.deepEqual(marketplace.at(-1)['https://registry.test/'].packages, [])
  assert.deepEqual(stores.at(-1).packages, [])
  assert.equal(packages.at(-1), null)
  assert.equal(legacy.selections, undefined)

  f.replace('/registryCache', { 'https://registry.test/': registryEntry() })
  assert.equal(marketplace.at(-1)['https://registry.test/'].packages.length, 1)
  assert.equal(stores.at(-1).packages[0].version, '1.0.0:0')
  await tick()
  assert.equal(packages.at(-1).version, '1.0.0:0')
  assert.deepEqual(f.queries, [])
})

test('marketplace lists the newest release and the update target from the index without long descriptions', async t => {
  const entry = registryEntry('3.0.0:0')
  entry.selections.example.versions = {
    '1.0.0:0': { s9pks: [0], superseded: true, compatible: true },
    '2.0.0:0': { s9pks: [0], compatible: true },
    '3.0.0:0': { s9pks: [0], compatible: true },
  }
  entry.selections.example.updateTarget = '2.0.0:0'
  const index = registryIndex()
  Object.assign(index.package.packages.example.versions['2.0.0:0'], {
    description: { short: '', long: 'Stepping stone details' },
    releaseNotes: 'Stepping stone',
  })
  const f = marketplaceFixture(t, { 'https://registry.test/': entry }, index)
  await f.service.connect('https://registry.test')
  const store = await firstValueFrom(f.service.currentRegistry$)
  assert.deepEqual(
    store.packages.map(({ version }) => version),
    ['3.0.0:0'],
  )
  assert.deepEqual(
    store.updates.map(({ version }) => version),
    ['2.0.0:0'],
  )
  assert.equal(store.updates[0].releaseNotes, 'Stepping stone')
  assert.equal(store.updates[0].description.long, undefined)
  const newest = await firstValueFrom(
    f.service.getPackage$('example', null, null),
  )
  assert.equal(newest.version, '3.0.0:0')
  assert.deepEqual(newest.otherVersions, {
    '1.0.0:0': { superseded: true },
    '2.0.0:0': {},
  })
  const target = await firstValueFrom(
    f.service.getPackage$('example', '2.0.0:0', null),
  )
  assert.equal(target.description.long, 'Stepping stone details')
  assert.deepEqual(f.queries, [])
})

test('registry asset positions keep trust, install pins and static files on the selected hardware', async t => {
  const entry = registryEntry()
  entry.selections.example.versions['1.0.0:0'].s9pks = [1]
  const index = registryIndex(['1.0.0:0'])
  const arm = [
    { arch: ['aarch64'] },
    {
      signatures: { [untrustedSigner]: 'arm-signature' },
      urls: ['https://assets.test/arm.s9pk'],
      commitment: { rootSighash: 'arm-root', rootMaxsize: 10 },
    },
  ]
  const native = [
    { arch: ['x86_64'] },
    {
      signatures: { [OS_UPDATE_SIGNERS[0]]: 'native-signature' },
      urls: ['https://assets.test/native.s9pk'],
      commitment: { rootSighash: 'native-root', rootMaxsize: 20 },
    },
  ]
  index.package.packages.example.versions['1.0.0:0'].s9pks = [arm, native]
  const f = marketplaceFixture(t, { 'https://registry.test/': entry }, index)
  const api = f.injector.get(ApiService)
  const submissions = []
  const files = []
  api.rpc.package = { install: async params => submissions.push(params) }
  api.getStatic = async (urls, commitment) => {
    files.push({ urls, commitment })
    return 'instructions'
  }
  f.replace('/serverInfo/trustedKeys', trustedSigners)
  await f.service.connect('https://registry.test')
  const store = await firstValueFrom(f.service.currentRegistry$)
  assert.deepEqual(store.packages[0].s9pks, [native])
  const details = await firstValueFrom(
    f.service.getDetails$('example', '1.0.0:0', 'https://registry.test/'),
  )
  assert.deepEqual(details.s9pks, [native])
  assert.deepEqual(details.best['1.0.0:0'].s9pks, [native])
  await f.service.installPackage(details, 'https://registry.test')
  assert.deepEqual(submissions[0].acceptSigners, {
    any: [{ signer: OS_UPDATE_SIGNERS[0] }],
  })
  assert.equal(
    await firstValueFrom(f.service.fetchStatic$(details, 'instructions.md')),
    'instructions',
  )
  assert.deepEqual(files, [
    {
      urls: [
        '/s9pk/proxy/https%3A%2F%2Fassets.test%2Fnative.s9pk/instructions.md',
      ],
      commitment: { rootSighash: 'native-root', rootMaxsize: 20 },
    },
  ])
  assert.deepEqual(index.package.packages.example.versions['1.0.0:0'].s9pks, [
    arm,
    native,
  ])
})

test("details carry every other version's release notes from the index, which a release alone omits", async t => {
  const entry = registryEntry()
  entry.selections.example.versions['0.9.0:0'] = {
    s9pks: [0],
    compatible: true,
  }
  const index = registryIndex(['0.9.0:0', '1.0.0:0'])
  Object.assign(index.package.packages.example.versions['0.9.0:0'], {
    releaseNotes: 'Older notes',
    defaultLocale: 'en_US',
  })
  const f = marketplaceFixture(t, { 'https://registry.test/': entry }, index)
  const details = await firstValueFrom(
    f.service
      .getDetails$('example', '1.0.0:0', 'https://registry.test/')
      .pipe(filter(Boolean)),
  )
  assert.deepEqual(details.otherVersions['0.9.0:0'], {
    releaseNotes: 'Older notes',
    defaultLocale: 'en_US',
  })
  await f.service.connect('https://registry.test')
  const release = await firstValueFrom(
    f.service.getPackage$('example', '1.0.0:0', null).pipe(filter(Boolean)),
  )
  assert.deepEqual(release.otherVersions['0.9.0:0'], {})
  assert.deepEqual(f.queries, [])
})

test('expanded details follow live index changes to every version', async t => {
  const entry = registryEntry('2.0.0:0')
  entry.selections.example.versions['1.5.0:0'] = {
    s9pks: [0],
    compatible: true,
  }
  const index = registryIndex(['1.5.0:0', '2.0.0:0'])
  index.package.packages.example.versions['2.0.0:0'].releaseNotes = 'Before'
  index.package.packages.example.versions['1.5.0:0'].releaseNotes =
    'Intermediate'
  const f = marketplaceFixture(t, { 'https://registry.test/': entry }, index)
  const values = f.watch(
    f.service.getDetails$('example', '2.0.0:0', 'https://registry.test/'),
  )
  assert.equal(values.at(-1).releaseNotes, 'Before')
  assert.equal(
    values.at(-1).otherVersions['1.5.0:0'].releaseNotes,
    'Intermediate',
  )
  f.replace(`${indexedRelease('2.0.0:0')}/releaseNotes`, 'After')
  f.replace(`${indexedRelease('1.5.0:0')}/releaseNotes`, 'Edited')
  assert.equal(values.at(-1).releaseNotes, 'After')
  assert.equal(values.at(-1).otherVersions['1.5.0:0'].releaseNotes, 'Edited')
  assert.deepEqual(f.queries, [])
})

test('an unlisted exact version is read from the index and follows its live signatures', async t => {
  const entry = registryEntry('2.0.0:0')
  entry.selections.example.versions['1.0.0:0'] = {
    s9pks: [0],
    compatible: true,
  }
  const index = registryIndex(['1.0.0:0', '2.0.0:0'])
  const s9pks = signatures => [[{}, { signatures }]]
  index.package.packages.example.versions['1.0.0:0'].s9pks = s9pks({
    first: 'signature',
  })
  const f = marketplaceFixture(t, { 'https://registry.test/': entry }, index)
  await f.service.connect('https://registry.test')
  const store = await firstValueFrom(f.service.currentRegistry$)
  assert.deepEqual(
    store.packages.map(({ version }) => version),
    ['2.0.0:0'],
  )
  const values = f.watch(f.service.getPackage$('example', '1.0.0:0', null))
  assert.deepEqual(Object.keys(values.at(-1).s9pks[0][1].signatures), ['first'])
  f.replace(
    `${indexedRelease('1.0.0:0')}/s9pks`,
    s9pks({ first: 'signature', second: 'signature' }),
  )
  assert.deepEqual(Object.keys(values.at(-1).s9pks[0][1].signatures), [
    'first',
    'second',
  ])
  assert.deepEqual(f.queries, [])
})

test('an exact version resolves while its registry selects it and the index holds it', async t => {
  const f = marketplaceFixture(t)
  await f.service.connect('https://registry.test')
  const values = f.watch(f.service.getPackage$('example', '1.0.0:0', null))
  assert.deepEqual(values, [])
  const entry = registryEntry('2.0.0:0')
  entry.selections.example.versions['1.0.0:0'] = {
    s9pks: [0],
    compatible: true,
  }
  f.replace('/registryCache', { 'https://registry.test/': entry })
  assert.equal(values.at(-1).version, '1.0.0:0')
  assert.equal(values.at(-1).title, '1.0.0:0')
  f.replace('/registryCache', {
    'https://registry.test/': registryEntry('3.0.0:0'),
  })
  assert.equal(values.at(-1), null)
  f.replace('/registryCache', { 'https://registry.test/': entry })
  assert.equal(values.at(-1).version, '1.0.0:0')
  f.replace(
    '/registryIndexes/https:~1~1registry.test~1/package/packages/example/versions',
    registryIndex(['2.0.0:0']).package.packages.example.versions,
  )
  assert.equal(values.at(-1), null)
  assert.deepEqual(f.queries, [])
})

function fakeIndexedDb() {
  let data = { values: new Map(), meta: new Map() }
  const control = { fail: false, puts: [] }
  const db = {
    transaction(_, mode) {
      const staged = { values: new Map(data.values), meta: new Map(data.meta) }
      const tx = {
        objectStore: name => ({
          get: key => {
            const request = { result: staged[name].get(key) }
            queueMicrotask(() => request.onsuccess?.())
            return request
          },
          getAll: () => ({ result: [...staged[name].values()] }),
          getAllKeys: () => ({ result: [...staged[name].keys()] }),
          put: (value, key) => {
            if (name === 'values') control.puts.push(key)
            staged[name].set(key, structuredClone(value))
          },
          delete: key => staged[name].delete(key),
          clear: () => staged[name].clear(),
        }),
      }
      queueMicrotask(() => {
        queueMicrotask(() => {
          if (mode === 'readwrite' && control.fail) {
            control.fail = false
            tx.error = new Error('quota')
            tx.onerror()
          } else {
            if (mode === 'readwrite') data = staged
            tx.oncomplete()
          }
        })
      })
      return tx
    },
  }
  const win = {
    indexedDB: {
      open: () => {
        const request = {}
        queueMicrotask(() => {
          request.result = db
          request.onsuccess()
        })
        return request
      },
    },
  }
  return { win, control }
}

async function rawPatchDbCache(t, win) {
  const { PatchDbCache } =
    await import('src/app/services/patch-db/patch-db-cache')
  const injector = createEnvironmentInjector([
    { provide: WA_WINDOW, useValue: win },
  ])
  t.after(() => injector.destroy())
  return runInInjectionContext(injector, () => new PatchDbCache())
}

const ROOTS = { '/ui': { epoch: 'boot', id: 1 } }

async function patchDbCache(t, win) {
  const cache = await rawPatchDbCache(t, win)
  return {
    roots: ROOTS,
    read: async () => (await cache.read()).dump,
    write(dump) {
      return cache.write(dump, this.roots)
    },
    markSynced: () => cache.markSynced(),
    clear: () => cache.clear(),
    synced: () => cache.synced(),
  }
}

test('patch cache rewrites only replaced top-level values', async t => {
  const { win, control } = fakeIndexedDb()
  t.mock.method(console, 'error', () => {})
  const writer = await patchDbCache(t, win)
  assert.deepEqual(await writer.read(), { id: 0, value: {} })
  await writer.write({ id: 3, value: { ui: { a: 1 }, packageData: {} } })
  assert.deepEqual(control.puts, ['/ui', '/packageData'])

  const reader = await patchDbCache(t, win)
  const cached = await reader.read()
  assert.deepEqual(cached, {
    id: 3,
    value: { ui: { a: 1 }, packageData: {} },
  })
  control.puts = []
  await reader.write({
    id: 4,
    value: { ...cached.value, packageData: { x: 1 } },
  })
  assert.deepEqual(control.puts, ['/packageData/x'])
  await reader.write({ id: 5, value: { ui: cached.value.ui } })
  assert.deepEqual(await (await patchDbCache(t, win)).read(), {
    id: 5,
    value: { ui: { a: 1 } },
  })
})

test('patch cache stores keyed maps one entry per child', async t => {
  const { win, control } = fakeIndexedDb()
  const writer = await patchDbCache(t, win)
  const alpha = { title: 'Alpha' }
  const beta = { title: 'Beta' }
  await writer.write({
    id: 3,
    value: { ui: { a: 1 }, packageData: { 'a/b': alpha, 'b~c': beta } },
  })
  assert.deepEqual(control.puts, [
    '/ui',
    '/packageData',
    '/packageData/a~1b',
    '/packageData/b~0c',
  ])
  const reader = await patchDbCache(t, win)
  const cached = await reader.read()
  assert.deepEqual(cached.value.packageData, { 'a/b': alpha, 'b~c': beta })
  control.puts = []
  const { 'b~c': _, ...rest } = cached.value.packageData
  await reader.write({
    id: 4,
    value: {
      ...cached.value,
      packageData: { ...rest, 'a/b': { title: 'A2' } },
    },
  })
  assert.deepEqual(control.puts, ['/packageData/a~1b'])
  assert.deepEqual((await (await patchDbCache(t, win)).read()).value, {
    ui: { a: 1 },
    packageData: { 'a/b': { title: 'A2' } },
  })
})

test('patch cache never persists registry or OS update data, or their cursors', async t => {
  const { win, control } = fakeIndexedDb()
  const cache = await rawPatchDbCache(t, win)
  const roots = {
    '/serverInfo': { epoch: 'boot', id: 3 },
    '/registryCache': { epoch: 'boot', id: 3 },
    '/registryIndexes': { epoch: 'boot', id: 3 },
    '/osUpdateCache': { epoch: 'boot', id: 3 },
  }
  await cache.write(
    {
      id: 3,
      value: {
        serverInfo: { a: 1 },
        registryCache: { 'https://a.test/': {} },
        registryIndexes: { 'https://a.test/': {} },
        osUpdateCache: { versions: {} },
      },
    },
    roots,
  )
  assert.deepEqual(control.puts, ['/serverInfo'])
  assert.deepEqual(await (await rawPatchDbCache(t, win)).read(), {
    dump: { id: 3, value: { serverInfo: { a: 1 } } },
    cursors: { '/serverInfo': { epoch: 'boot', id: 3 } },
  })
})

test('whole-document subscriptions retain offline coverage without persisting external caches', async t => {
  const { win } = fakeIndexedDb()
  const cache = await rawPatchDbCache(t, win)
  const cursor = { epoch: 'boot', id: 5 }
  const projected = selectorKey({ include: '/ui', depth: 2 })
  const value = {
    serverInfo: { hostname: 'cached' },
    packageData: {},
    ui: { a: 1 },
    registryCache: { r: {} },
    registryIndexes: { r: {} },
    osUpdateCache: { versions: {} },
  }
  await cache.write(
    { id: 5, value },
    {
      '': cursor,
      [projected]: cursor,
      [selectorKey({ include: '/registryIndexes/r', exclude: ['/x'] })]: cursor,
    },
  )
  const { dump, cursors } = await (await rawPatchDbCache(t, win)).read()
  assert.deepEqual(cursors, {
    '/serverInfo': cursor,
    '/packageData': cursor,
    '/ui': cursor,
    [projected]: cursor,
  })
  assert.deepEqual(Object.keys(dump.value).sort(), [
    'packageData',
    'serverInfo',
    'ui',
  ])
  const patch = new SubtreePatchDB({ subscribe: () => EMPTY })
  patch.restore(dump, cursors)
  const values = []
  const subscription = patch.watch$('serverInfo').subscribe(v => values.push(v))
  t.after(() => {
    subscription.unsubscribe()
    patch.stop()
  })
  assert.deepEqual(values, [{ hostname: 'cached' }])
})

test('patch cache stores a keyed root that is not a map whole', async t => {
  const { win, control } = fakeIndexedDb()
  const cache = await patchDbCache(t, win)
  await cache.write({ id: 3, value: { packageData: { x: 1 } } })
  await cache.write({ id: 4, value: { packageData: null } })
  assert.deepEqual(await (await patchDbCache(t, win)).read(), {
    id: 4,
    value: { packageData: null },
  })
  control.puts = []
  await cache.write({ id: 5, value: { packageData: { y: 1 } } })
  assert.deepEqual(control.puts, ['/packageData', '/packageData/y'])
})

test(
  'a blocked patch cache upgrade yields an empty baseline without waiting for the old tab',
  { timeout: 1000 },
  async t => {
    t.mock.method(console, 'error', () => {})
    const request = {}
    const win = {
      indexedDB: {
        open: () => {
          queueMicrotask(() => request.onblocked())
          return request
        },
      },
    }
    const cache = await patchDbCache(t, win)
    assert.deepEqual(await cache.read(), { id: 0, value: {} })

    let closed = false
    request.result = {
      close: () => {
        closed = true
      },
    }
    request.onsuccess()
    assert.equal(closed, true)
  },
)

test('patch cache releases its connection for a later schema upgrade', async t => {
  const { win } = fakeIndexedDb()
  const open = win.indexedDB.open
  let db
  let closed = false
  win.indexedDB.open = (...args) => {
    const request = open(...args)
    queueMicrotask(() => {
      db = request.result
      db.close = () => {
        closed = true
      }
    })
    return request
  }
  const cache = await patchDbCache(t, win)
  await cache.read()
  db.onversionchange()
  assert.equal(closed, true)
})

test('patch cache drops a store written in the top-level layout', async t => {
  const { win } = fakeIndexedDb()
  const calls = []
  const open = win.indexedDB.open
  win.indexedDB.open = (name, version) => {
    calls.push(['open', version])
    let request
    queueMicrotask(() => {
      request.result = {
        deleteObjectStore: store => calls.push(['delete', store]),
        createObjectStore: store => calls.push(['create', store]),
      }
      request.onupgradeneeded({ oldVersion: 1 })
    })
    request = open(name, version)
    return request
  }
  await (await patchDbCache(t, win)).read()
  assert.deepEqual(calls, [
    ['open', 3],
    ['delete', 'values'],
    ['delete', 'meta'],
    ['create', 'values'],
    ['create', 'meta'],
  ])
})

test('patch cache keeps a complete snapshot when another tab advances the stored revision', async t => {
  const { win } = fakeIndexedDb()
  const fast = await patchDbCache(t, win)
  const slow = await patchDbCache(t, win)
  await fast.write({ id: 3, value: { ui: { a: 1 }, packageData: {} } })
  const cached = await slow.read()
  await fast.write({
    id: 5,
    value: { ui: { a: 3 }, packageData: { x: 1 } },
  })
  const delayed = {
    id: 4,
    value: { ui: { a: 2 }, packageData: cached.value.packageData },
  }
  await slow.write(delayed)
  assert.deepEqual(await (await patchDbCache(t, win)).read(), delayed)
})

test('patch cache rewrites every key when another tab stored the same revision from another epoch', async t => {
  const { win } = fakeIndexedDb()
  const rebooted = await patchDbCache(t, win)
  const stale = await patchDbCache(t, win)
  rebooted.roots = { '/ui': { epoch: 'boot-1', id: 3 } }
  await rebooted.write({ id: 3, value: { ui: { a: 1 }, packageData: {} } })
  const cached = await stale.read()
  rebooted.roots = { '/ui': { epoch: 'boot-2', id: 3 } }
  await rebooted.write({
    id: 3,
    value: { ui: { a: 9 }, packageData: { x: 9 } },
  })
  const delayed = {
    id: 4,
    value: { ui: { a: 2 }, packageData: cached.value.packageData },
  }
  await stale.write(delayed)
  const reader = await patchDbCache(t, win)
  assert.deepEqual(await reader.read(), delayed)
})

test('patch cache restores every key after another tab clears the snapshot', async t => {
  const { win } = fakeIndexedDb()
  const active = await patchDbCache(t, win)
  const other = await patchDbCache(t, win)
  const value = { ui: { a: 1 }, packageData: { x: 1 } }
  await active.write({ id: 3, value })
  await other.clear()
  const next = { id: 4, value: { ...value, ui: { a: 2 } } }
  await active.write(next)
  assert.deepEqual(await (await patchDbCache(t, win)).read(), next)
})

test('patch cache restores freshness with a saved snapshot and clears it on logout', async t => {
  const { win } = fakeIndexedDb()
  t.mock.method(Date, 'now', () => 1000)
  const cache = await patchDbCache(t, win)
  await cache.markSynced()
  assert.equal(cache.synced(), null)
  const emptyReader = await patchDbCache(t, win)
  assert.deepEqual(await emptyReader.read(), { id: 0, value: {} })
  assert.equal(emptyReader.synced(), null)

  const snapshot = { id: 3, value: { ui: { a: 1 } } }
  await cache.write(snapshot)
  await cache.markSynced()
  assert.equal(cache.synced(), 1000)
  const reader = await patchDbCache(t, win)
  assert.deepEqual(await reader.read(), snapshot)
  assert.equal(reader.synced(), 1000)

  await cache.clear()
  assert.equal(cache.synced(), null)
  assert.deepEqual(await reader.read(), { id: 0, value: {} })
  assert.equal(reader.synced(), null)
})

test('patch cache discards freshness when a failed write invalidates its snapshot', async t => {
  const { win, control } = fakeIndexedDb()
  t.mock.method(console, 'error', () => {})
  t.mock.method(Date, 'now', () => 1000)
  const cache = await patchDbCache(t, win)
  await cache.write({ id: 3, value: { ui: { a: 1 } } })
  await cache.markSynced()
  control.fail = true
  await cache.write({ id: 4, value: { ui: { a: 2 } } })
  assert.equal(cache.synced(), null)
  const reader = await patchDbCache(t, win)
  assert.deepEqual(await reader.read(), { id: 0, value: {} })
  assert.equal(reader.synced(), null)
})

test('patch cache does not refresh metadata after another tab clears the snapshot', async t => {
  const { win } = fakeIndexedDb()
  let now = 1000
  t.mock.method(Date, 'now', () => now)
  const active = await patchDbCache(t, win)
  await active.write({ id: 3, value: { ui: { a: 1 } } })
  await active.markSynced()
  const other = await patchDbCache(t, win)
  await other.clear()
  now = 2000
  await active.markSynced()
  assert.equal(active.synced(), 1000)
  const reader = await patchDbCache(t, win)
  assert.deepEqual(await reader.read(), { id: 0, value: {} })
  assert.equal(reader.synced(), null)
})

test('patch cache retains the previous freshness when its metadata write fails', async t => {
  const { win, control } = fakeIndexedDb()
  let now = 1000
  t.mock.method(console, 'error', () => {})
  t.mock.method(Date, 'now', () => now)
  const cache = await patchDbCache(t, win)
  const snapshot = { id: 3, value: { ui: { a: 1 } } }
  await cache.write(snapshot)
  await cache.markSynced()
  now = 2000
  control.fail = true
  await cache.markSynced()
  assert.equal(cache.synced(), 1000)
  const reader = await patchDbCache(t, win)
  assert.deepEqual(await reader.read(), snapshot)
  assert.equal(reader.synced(), 1000)
})

test('patch cache discards its snapshot after a failed write and clears on demand', async t => {
  const { win, control } = fakeIndexedDb()
  t.mock.method(console, 'error', () => {})
  const cache = await patchDbCache(t, win)
  const value = { ui: { a: 1 }, packageData: {} }
  await cache.write({ id: 3, value })
  control.fail = true
  await cache.write({ id: 4, value: { ...value, ui: { a: 2 } } })
  assert.deepEqual(await (await patchDbCache(t, win)).read(), {
    id: 0,
    value: {},
  })
  control.puts = []
  await cache.write({ id: 5, value })
  assert.deepEqual(control.puts, ['/ui', '/packageData'])
  await cache.clear()
  assert.deepEqual(await (await patchDbCache(t, win)).read(), {
    id: 0,
    value: {},
  })
})
