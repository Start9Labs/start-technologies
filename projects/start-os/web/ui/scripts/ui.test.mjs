import assert from 'node:assert/strict'
import { generateKeyPairSync, webcrypto } from 'node:crypto'
import { readFileSync, existsSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { resolve } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import test from 'node:test'
import ts from 'typescript'
import '@angular/compiler'
import {
  createEnvironmentInjector,
  runInInjectionContext,
  signal,
} from '@angular/core'
import { BehaviorSubject, EMPTY, map, Subject } from 'rxjs'

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
    'LocalizePipe',
    'MarkdownPipe',
    'SafeLinksDirective',
    'RPCErrorDetails',
  ],
  'export const LANGUAGES = []; export const LANGUAGE_TO_CODE = {}; export const getKeyboardName = () => ""; export const getAllKeyboardsSorted = () => []; export const pauseFor = async () => {};',
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
    'TuiGroup',
    'TuiCheckbox',
    'TuiLoader',
  ],
  'export const tuiCellOptionsProvider = () => [];',
)
boundary('@taiga-ui/kit', [
  'TuiBadge',
  'TuiBadgeNotification',
  'TuiButtonLoading',
  'TuiButtonSelect',
  'TuiDataListWrapper',
  'TuiBlock',
  'TuiRadioList',
])
boundary(
  '@taiga-ui/cdk',
  ['TuiAnimated', 'TuiMapperPipe'],
  'export const tuiProvide = () => [];',
)
boundary(
  '@taiga-ui/i18n',
  ['TuiLanguageSwitcherService'],
  'export const tuiLanguageSwitcher = () => [];',
)
boundary('@taiga-ui/dompurify', ['NgDompurifyPipe'])
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
boundary('@start9labs/marketplace', [], 'export const FALLBACK_ICON = "";')
boundary(
  'src/app/services/patch-db/patch-db-source',
  [],
  'export const PATCH_CACHE = Symbol("patch cache");',
)
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
    if (url.endsWith('.md'))
      return {
        format: 'module',
        source: `export default ${JSON.stringify(readFileSync(new URL(url), 'utf8'))}`,
        shortCircuit: true,
      }
    if (url.endsWith('.ts'))
      return {
        format: 'module',
        source: `import { createRequire as __createRequire } from 'node:module'; const require = __createRequire(import.meta.url);\n${
          ts.transpileModule(readFileSync(new URL(url), 'utf8'), {
            compilerOptions,
            fileName: fileURLToPath(url),
          }).outputText
        }`,
        shortCircuit: true,
      }
    return next(url, context)
  },
})
const { PatchDB } = await import('patch-db-client')
const { OSService } = await import('src/app/services/os.service')
const { i18nService: ProductionI18nService } = await import(
  pathToFileURL(
    resolve(root, 'shared-libs/ts-modules/shared/src/i18n/i18n.service.ts'),
  ).href
)
const { LocalizePipe: ProductionLocalizePipe } = await import(
  pathToFileURL(
    resolve(root, 'shared-libs/ts-modules/shared/src/i18n/localize.pipe.ts'),
  ).href
)
const { I18N } = await import(
  pathToFileURL(
    resolve(root, 'shared-libs/ts-modules/shared/src/i18n/i18n.providers.ts'),
  ).href
)
const { ApiService } = await import('src/app/services/api/embassy-api.service')
const { TaskService, DialogService, i18nPipe, i18nService } =
  await import('@start9labs/shared')
const { CONTEXT } = await import('@taiga-ui/polymorpheus')
const { WA_WINDOW } = await import('@ng-web-apis/common')
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
function fixture(t, cachedVersion) {
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

test('trusted signer matching equates legacy PEM and compact DER64', () => {
  const pem =
    '-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAgpcI3hOExv1njCt+zJoYBartdyk4SKiqSIy3+9tcpOI=\n-----END PUBLIC KEY-----\n'
  const der64 = 'MCowBQYDK2VwAyEAgpcI3hOExv1njCt+zJoYBartdyk4SKiqSIy3+9tcpOI='
  assert.deepEqual(
    trustedOsUpdateSigners([pem], { [der64]: { label: 'signer' } }),
    [pem],
  )
  assert.deepEqual(
    trustedOsUpdateSigners([der64], { [pem]: { label: 'signer' } }),
    [der64],
  )
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
const { getManifest, getCurrentManifest } =
  await import('src/app/utils/get-package-data')
const { Router } = await import('@angular/router')
const volumeA = '7cb12a88-783d-4909-9923-d5eb47c7e01f'
const volumeB = '9c6c47e8-1f98-4981-a92a-23fcf4bb7a82'

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
    grants = []
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
        async run(fn) {
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
    get completed() {
      return completed
    },
    make: Class => runInInjectionContext(injector, () => new Class()),
  }
}

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
    ['Off', null],
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
