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
import { BehaviorSubject, EMPTY, of, Subject } from 'rxjs'
import { FormControl } from '@angular/forms'

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
    'ErrorService',
    'Exver',
    'ExverComparesPipe',
  ],
  'export const LANGUAGES = []; export const LANGUAGE_TO_CODE = {}; export const getKeyboardName = () => ""; export const getAllKeyboardsSorted = () => []; export const FALLBACK_ICON = "fallback.png"; export const defaultRegistries = { start9: "https://registry.test", community: "https://community.test" }; export const sameUrl = (a, b) => a === b; export const registryUrl = value => value;',
)
boundary(
  '@taiga-ui/core',
  [
    'TuiButton',
    'TuiScrollbar',
    'TuiAppearance',
    'TuiCell',
    'TuiDataList',
    'TuiDropdown',
    'TuiNotification',
    'TuiLoader',
    'TuiError',
    'TuiIcon',
    'TuiInput',
    'TuiTitle',
  ],
  'export const tuiCellOptionsProvider = () => []; export const tuiValidationErrorsProvider = () => [];',
)
boundary('@taiga-ui/kit', [
  'TuiBadge',
  'TuiBadgeNotification',
  'TuiButtonLoading',
  'TuiButtonSelect',
  'TuiDataListWrapper',
  'TuiSwitch',
  'TuiTextarea',
])
boundary(
  '@taiga-ui/cdk',
  ['TuiAnimated'],
  'export const tuiMarkControlAsTouchedAndValidate = () => {};',
)
boundary('@taiga-ui/layout', ['TuiHeader', 'TuiForm'])
boundary('@taiga-ui/dompurify', ['NgDompurifyPipe'])
boundary(
  '@ng-web-apis/common',
  [],
  'export const WA_WINDOW = Symbol("window");',
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
const root = fileURLToPath(new URL('../../../../../', import.meta.url))
const ui = resolve(root, 'projects/start-os/web/ui')
boundaries.set(
  '@start9labs/marketplace',
  ['trust.ts', 'types.ts', 'services/abstract-marketplace.service.ts']
    .map(
      file =>
        `export * from ${JSON.stringify(pathToFileURL(resolve(root, 'shared-libs/ts-modules/marketplace/src', file)).href)};`,
    )
    .join('\n'),
)
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
const { PatchDB } = await import('patch-db-client')
const { OSService } = await import('src/app/services/os.service')
const { ApiService } = await import('src/app/services/api/embassy-api.service')
const { TaskService, DialogService, i18nPipe, i18nService } =
  await import('@start9labs/shared')
const { CONTEXT } = await import('@taiga-ui/polymorpheus')
const { WA_WINDOW } = await import('@ng-web-apis/common')
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
    resolve(
      root,
      'shared-libs/crates/start-core/src/sign/fixtures/ed25519-signers.asc',
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

const { TrustedKeysDialog } =
  await import('src/app/components/trusted-keys/dialog')
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
        .sort(TrustedKeysDialog.prototype.compareKeyrings)
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

test('a successful trust toggle leaves the control to the requested value and PatchDB updates', async () => {
  const control = new FormControl(true)
  const viewWrites = []
  control.registerOnChange(value => viewWrites.push(value))
  const mutations = []
  const instance = {
    context: { data: 'marketplace' },
    saving: { set() {} },
    trust: { keys: () => ({ publisher: { marketplace: false } }) },
    api: {
      rpc: {
        server: {
          trustedKeys: { toggle: async params => mutations.push(params) },
        },
      },
    },
    tasks: {
      run: async work => {
        await work()
        return true
      },
    },
  }
  await TrustedKeysDialog.prototype.toggle.call(instance, 'publisher', true, {
    control,
  })
  assert.deepEqual(mutations, [
    { id: 'publisher', enabled: true, context: 'marketplace' },
  ])
  assert.equal(control.value, true)
  assert.deepEqual(viewWrites, [])
})

test('a rejected trust toggle restores the persisted value without emitting another mutation', async () => {
  const control = new FormControl(true)
  const viewWrites = []
  control.registerOnChange((value, emitModelEvent) =>
    viewWrites.push({ value, emitModelEvent }),
  )
  const saving = []
  const instance = {
    context: { data: 'marketplace' },
    saving: { set: value => saving.push(value) },
    trust: { keys: () => ({ publisher: { marketplace: false } }) },
    api: {
      rpc: {
        server: {
          trustedKeys: {
            toggle: async () => {
              throw new Error('offline')
            },
          },
        },
      },
    },
    tasks: {
      run: async work => {
        try {
          await work()
          return true
        } catch {
          return false
        }
      },
    },
  }
  await TrustedKeysDialog.prototype.toggle.call(instance, 'publisher', true, {
    control,
  })
  assert.equal(control.value, false)
  assert.deepEqual(viewWrites, [{ value: false, emitModelEvent: false }])
  assert.deepEqual(saving, [true, false])
})
