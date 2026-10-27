import assert from 'node:assert/strict'
import { generateKeyPairSync } from 'node:crypto'
import { readFileSync, existsSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import test from 'node:test'
import ts from 'typescript'
import '@angular/compiler'
import { createEnvironmentInjector, runInInjectionContext } from '@angular/core'
import { BehaviorSubject, EMPTY, Subject } from 'rxjs'

// Presentation substitutes keep browser-only dependencies out of class-method tests.
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
    'i18nPipe',
    'i18nService',
    'MarkdownPipe',
    'SafeLinksDirective',
  ],
  'export const LANGUAGES = []; export const LANGUAGE_TO_CODE = {}; export const getKeyboardName = () => ""; export const getAllKeyboardsSorted = () => [];',
)
boundary(
  '@taiga-ui/core',
  [
    'TuiButton',
    'TuiScrollbar',
    'TuiAppearance',
    'TuiCell',
    'TuiIcon',
    'TuiInput',
    'TuiTitle',
    'TuiNotification',
  ],
  'export const tuiCellOptionsProvider = () => [];',
)
boundary('@taiga-ui/kit', [
  'TuiBadge',
  'TuiBadgeNotification',
  'TuiButtonLoading',
  'TuiButtonSelect',
  'TuiDataListWrapper',
])
boundary('@taiga-ui/cdk', ['TuiAnimated'])
boundary('@taiga-ui/addon-mobile', ['TuiResponsiveDialog'])
boundary('@taiga-ui/dompurify', ['NgDompurifyPipe'])
boundary(
  '@ng-web-apis/common',
  [],
  'export const WA_WINDOW = Symbol("window"); export const WA_SESSION_STORAGE = Symbol("session storage");',
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
boundary('src/app/services/config.service', ['ConfigService'])
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
const root = fileURLToPath(new URL('../../../../../', import.meta.url))
const ui = resolve(root, 'projects/start-os/web/ui')
const options = ts.readConfigFile(
  resolve(root, 'tsconfig.json'),
  ts.sys.readFile,
).config.compilerOptions
const compilerOptions = ts.convertCompilerOptionsFromJson(options, root).options
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
        if (existsSync(candidate))
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
const { SwUpdate } = await import('@angular/service-worker')
const { RefreshAlertComponent } =
  await import('src/app/components/refresh-alert.component')
const { PatchDB } = await import('patch-db-client')
const { OSService } = await import('src/app/services/os.service')
const { ApiService } = await import('src/app/services/api/embassy-api.service')
const { TaskService, DialogService, i18nPipe, i18nService } =
  await import('@start9labs/shared')
const { CONTEXT } = await import('@taiga-ui/polymorpheus')
const { WA_WINDOW, WA_SESSION_STORAGE } = await import('@ng-web-apis/common')
const { UiUpdateService } = await import('src/app/services/ui-update.service')
const { ConfigService } = await import('src/app/services/config.service')
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
  OS_UPDATE_SIGNERS.map((key, i) => [key, { label: `Signer ${i}` }]),
)
const squashfs = {
  x86_64: { signatures: { [OS_UPDATE_SIGNERS[0]]: 'signature' } },
}
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
    trustedOsUpdateSigners: trustedSigners,
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
              path: '/serverInfo/trustedOsUpdateSigners',
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

function uiFixture(
  t,
  {
    enabled = true,
    controlled = true,
    storage = new Map(),
    build = 'build-a',
  } = {},
) {
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
      useValue: { version: '0.4.0.4', gitHash: build },
    },
    { provide: WA_WINDOW, useValue: win },
    {
      provide: WA_SESSION_STORAGE,
      useValue: storage && {
        getItem: key => storage.get(key) ?? null,
        setItem: (key, value) => storage.set(key, value),
      },
    },
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
    storage,
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

test('an OS version mismatch checks and tracks installation without blocking or premature reload', async t => {
  const f = uiFixture(t)
  assert.equal(f.ui.state().status, 'idle')
  assert.equal(f.checks.length, 0)
  f.version('0.4.0.5')
  assert.equal(f.ui.state().status, 'checking')
  assert.equal(f.checks.length, 1)
  await f.ui.check()
  assert.equal(f.checks.length, 1)
  f.ui.reload()
  assert.equal(f.reloads, 0)
  f.events.next(detected)
  assert.equal(f.ui.state().status, 'updating')
  f.ui.reload()
  assert.equal(f.reloads, 0)
  f.events.next(ready)
  f.checks[0].resolve(true)
  await tick()
  assert.equal(f.ui.state().status, 'ready')
  assert.equal(f.reloads, 0)
  assert.deepEqual(f.tasks, [])
  f.ui.reload()
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
    f.ui.reload()
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
  f.ui.reload()
  assert.equal(f.reloads, 1)
})

test('a false check without a no-new-version event is retryable, not falsely ready', async t => {
  const f = uiFixture(t)
  f.version('0.4.0.5')
  f.checks[0].resolve(false)
  await tick()
  assert.deepEqual(f.ui.state(), { status: 'error', error: null })
  f.ui.reload()
  assert.equal(f.reloads, 0)
})

test('no-new-version events do not announce an update when the running interface matches the server', t => {
  const f = uiFixture(t)
  const component = f.make(RefreshAlertComponent)
  f.events.next(noUpdate)
  assert.equal(f.ui.state().status, 'idle')
  assert.equal(component.show(), false)
})

for (const failure of ['installation', 'unrecoverable']) {
  test(`${failure} automatically reloads once per UI build in the tab, then exposes repeated failures`, async t => {
    const storage = new Map()
    const f = uiFixture(t, { storage })
    f.version('0.4.0.5')
    const emit = fixture =>
      failure === 'installation'
        ? fixture.events.next(failed)
        : fixture.unrecoverable.next({
            type: 'UNRECOVERABLE_STATE',
            reason: 'missing cached file',
          })
    emit(f)
    assert.equal(f.reloads, 1)
    assert.equal(f.ui.state().status, 'error')
    emit(f)
    assert.equal(f.reloads, 1)
    f.checks[0].resolve(false)
    await tick()
    assert.equal(f.ui.state().status, 'error')
    const afterReload = uiFixture(t, { storage })
    emit(afterReload)
    assert.equal(afterReload.reloads, 0)
    assert.equal(afterReload.ui.state().status, 'error')
    const nextBuild = uiFixture(t, { storage, build: 'build-b' })
    emit(nextBuild)
    assert.equal(nextBuild.reloads, 1)
  })
}

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

test('a silent worker leaves a nonblocking checking state with an explicit cache-reset escape', async t => {
  const storage = new Map([
    ['auth', 'device-key'],
    ['settings', 'preferences'],
  ])
  const f = uiFixture(t, { storage })
  const component = f.make(RefreshAlertComponent)
  f.version('0.4.0.5')
  await tick()
  assert.equal(f.ui.state().status, 'checking')
  assert.deepEqual(f.tasks, [])
  component.resetCache()
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
  assert.deepEqual(
    [...storage],
    [
      ['auth', 'device-key'],
      ['settings', 'preferences'],
    ],
  )
  assert.equal(f.reloads, 1)
  f.checks[0].resolve(false)
  await tick()
  assert.equal(f.ui.state().status, 'resetting')
})

test('when session storage is unavailable, failure stays actionable rather than risking a reload loop', t => {
  const f = uiFixture(t, { storage: null })
  f.events.next(failed)
  assert.equal(f.ui.state().status, 'error')
  assert.equal(f.reloads, 0)
})

test('cache-reset failure preserves the UI and exposes the error without an automatic reload', async t => {
  const f = uiFixture(t)
  f.version('0.4.0.5')
  f.registration.unregister = async () => {
    throw new Error('browser refused reset')
  }
  await f.ui.resetCache()
  assert.equal(f.ui.state().status, 'error')
  assert.match(f.ui.state().error, /browser refused reset/)
  assert.equal(f.reloads, 0)
  assert.equal(f.caches.size, 5)
})

test('dismissing an update notice does not hide the next state', t => {
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

  f.trust({ [untrustedSigner]: { label: 'My custom release signer' } })
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
  assert.deepEqual(trustedOsUpdateSigners([key], trustedSigners), [key])
  assert.deepEqual(
    trustedOsUpdateSigners(['unrelated key'], trustedSigners),
    [],
  )
  assert.deepEqual(trustedOsUpdateSigners([], trustedSigners), [])
  assert.deepEqual(trustedOsUpdateSigners([key], {}), [])
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
