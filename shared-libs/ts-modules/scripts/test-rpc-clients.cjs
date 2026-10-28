const assert = require('node:assert/strict')
const fs = require('node:fs')
const path = require('node:path')
const test = require('node:test')
const vm = require('node:vm')
const ts = require('typescript')
const {
  BehaviorSubject,
  Observable,
  filter,
  map,
  of,
  throwError,
} = require('rxjs')

const root = path.resolve(__dirname, '../../..')
const config = ts.readConfigFile(
  path.join(root, 'tsconfig.json'),
  ts.sys.readFile,
)
assert.equal(config.error, undefined)
const compilerOptions = ts.parseJsonConfigFileContent(
  config.config,
  ts.sys,
  root,
).options

function harness(moduleOverrides = new Map()) {
  const dependencies = new Map()
  const tokens = Object.fromEntries(
    [
      'DOCUMENT',
      'Injector',
      'HttpClient',
      'HttpService',
      'AuthKeyService',
      'AuthService',
      'StaleUiService',
      'PATCH_CACHE',
      'RELATIVE_URL',
    ].map(name => [name, Symbol(name)]),
  )
  const modules = new Map()
  const angular = {
    DOCUMENT: tokens.DOCUMENT,
    Injector: tokens.Injector,
    Injectable: () => value => value,
    Component: () => value => value,
    signal: initial => {
      const stream$ = new BehaviorSubject(initial)
      return Object.assign(() => stream$.value, {
        set: next => stream$.next(next),
        stream$,
      })
    },
    input: Object.assign(initial => angular.signal(initial), {
      required: () => angular.signal(undefined),
    }),
    inject: token => {
      assert.ok(dependencies.has(token), `Missing injection: ${String(token)}`)
      return dependencies.get(token)
    },
  }
  function load(relative) {
    const filename = path.resolve(root, relative)
    if (modules.has(filename)) return modules.get(filename)
    const result = { exports: {} }
    modules.set(filename, result.exports)
    const code = ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
      fileName: filename,
      compilerOptions: { ...compilerOptions, module: ts.ModuleKind.CommonJS },
    }).outputText
    const shared = () => ({
      ...tokens,
      pauseFor: async () => {},
      ...load('shared-libs/ts-modules/shared/src/types/rpc.types.ts'),
      ...load('shared-libs/ts-modules/shared/src/classes/rpc-error.ts'),
    })
    const requireSource = request => {
      if (moduleOverrides.has(request)) return moduleOverrides.get(request)
      if (request === '@angular/core') return angular
      if (request === '@angular/common/http')
        return { HttpClient: tokens.HttpClient }
      if (request === '@start9labs/shared') return shared()
      if (request === '@start9labs/marketplace')
        return load('shared-libs/ts-modules/marketplace/src/types.ts')
      if (request.endsWith('auth.service'))
        return { AuthService: tokens.AuthService }
      if (request.endsWith('http.service'))
        return { HttpService: tokens.HttpService }
      if (request.endsWith('stale-ui.service'))
        return { StaleUiService: tokens.StaleUiService }
      if (request.endsWith('patch-db-source'))
        return { PATCH_CACHE: tokens.PATCH_CACHE }
      if (request.startsWith('src/')) {
        const app = filename.slice(
          0,
          filename.lastIndexOf(`${path.sep}src${path.sep}`),
        )
        return load(path.relative(root, path.join(app, `${request}.ts`)))
      }
      if (request.startsWith('.'))
        return load(
          path.relative(
            root,
            path.resolve(path.dirname(filename), `${request}.ts`),
          ),
        )
      return require(request)
    }
    vm.runInNewContext(
      code,
      {
        exports: result.exports,
        module: result,
        require: requireSource,
        console: { ...console, error() {} },
        Buffer,
        URL,
      },
      { filename },
    )
    return result.exports
  }
  dependencies.set(tokens.DOCUMENT, {
    location: { origin: 'https://router.test' },
    defaultView: { location: { host: 'server.test', protocol: 'https:' } },
  })
  dependencies.set(tokens.RELATIVE_URL, '/rpc/v1')
  return { dependencies, tokens, load }
}

for (const [name, file, className, deauthMethod] of [
  [
    'StartOS',
    'projects/start-os/web/ui/src/app/services/api/embassy-live-api.service.ts',
    'LiveApiService',
    'setUnverified',
  ],
  [
    'Tunnel',
    'projects/start-tunnel/web/src/app/services/api/live-api.service.ts',
    'LiveApiService',
    'deauthenticate',
  ],
]) {
  test(`${name} signs local calls, excludes foreign origins and waits for patches`, async () => {
    const { dependencies, tokens, load } = harness()
    const cache = new BehaviorSubject({ id: 1 })
    const calls = []
    const signed = []
    let patchSequence = '3'
    dependencies.set(tokens.PATCH_CACHE, cache)
    dependencies.set(tokens.AuthService, { deauthenticate() {} })
    dependencies.set(tokens.AuthKeyService, {
      signRpcHeaders: async options => {
        signed.push(options)
        return { signature: 'signed' }
      },
    })
    dependencies.set(tokens.HttpService, {
      rpcRequest: async (options, url) => {
        calls.push({ options, url })
        return {
          body: { result: null },
          headers: { get: () => patchSequence },
        }
      },
    })
    const Client = load(file)[className]
    assert.equal(typeof Client, 'function')
    const client = new Client()
    let settled = false
    const options = {
      method: 'auth.logout',
      params: {},
      headers: { existing: 'preserved' },
    }
    const pending = client.rpcRequest(options).then(value => {
      settled = true
      return value
    })
    await new Promise(resolve => setImmediate(resolve))
    assert.equal(settled, false)
    assert.equal(signed[0], options)
    assert.equal(calls[0].options.headers.existing, 'preserved')
    assert.equal(calls[0].options.headers.signature, 'signed')
    cache.next({ id: 2 })
    await new Promise(resolve => setImmediate(resolve))
    assert.equal(settled, false)
    cache.next({ id: 3 })
    assert.equal(await pending, null)
    patchSequence = null
    assert.equal(
      await client.rpcRequest(options, 'https://foreign.test/rpc'),
      null,
    )
    assert.equal(signed.length, 1)
    assert.equal(calls[1].options, options)
    assert.equal(calls[1].url, 'https://foreign.test/rpc')
    cache.complete()
  })

  test(`${name} deauthenticates code 34 before waiting on patches`, async () => {
    const { dependencies, tokens, load } = harness()
    let deauthenticated = 0
    dependencies.set(tokens.PATCH_CACHE, new BehaviorSubject({ id: 0 }))
    dependencies.set(tokens.AuthService, {
      [deauthMethod]: () => deauthenticated++,
    })
    dependencies.set(tokens.AuthKeyService, {
      signRpcHeaders: async () => ({}),
    })
    dependencies.set(tokens.HttpService, {
      rpcRequest: async () => ({
        body: {
          error: {
            code: 34,
            message: 'unauthorized',
            data: { details: 'session expired' },
          },
        },
        headers: { get: () => '999' },
      }),
    })
    const client = new (load(file)[className])()
    await assert.rejects(
      client.rpcRequest({ method: 'auth.logout', params: {} }),
      error =>
        error.code === 34 &&
        error.message === 'RPC ERROR: unauthorized\n\nsession expired',
    )
    assert.equal(deauthenticated, 1)
  })
}

test('StartOS derives the method path and forwards results untouched', async () => {
  const { load } = harness()
  const client = Object.create(
    load(
      'projects/start-os/web/ui/src/app/services/api/embassy-live-api.service.ts',
    ).LiveApiService.prototype,
  )
  const value = { serverInfo: { id: 'server' }, packageData: {} }
  const dump = { guid: 'subscription', dump: { id: 3, value } }
  const action = { version: '1', title: 'Done', message: null, result: null }
  const requests = []
  let response = dump
  client.rpcRequest = async options => {
    requests.push(options)
    return response
  }
  const rpc = load(
    'shared-libs/ts-modules/start-core/lib/rpcClient.ts',
  ).createRpcClient({
    call: (method, params, options) =>
      client.rpcRequest({ method, params, ...options }),
  })
  assert.equal(await rpc.db.subscribe({}), dump)
  assert.equal(requests[0].method, 'db.subscribe')
  assert.equal(Object.keys(requests[0].params).length, 0)
  response = action
  assert.equal(
    await rpc.package.action.run({ id: 'demo', actionId: 'test', input: null }),
    action,
  )
  assert.equal(requests[1].method, 'package.action.run')
  response = null
  assert.equal(
    await rpc.package.action.getInput({ id: 'demo', actionId: 'test' }),
    null,
  )
  assert.equal(requests[2].method, 'package.action.get-input')
  await rpc.server.logs({ limit: 1 }, { timeout: 10000 })
  assert.equal(requests[3].timeout, 10000)
})

test('Setup converts the actual JWK response and forwards the installation timeout', async () => {
  const { dependencies, tokens, load } = harness()
  const calls = []
  const key = {
    kty: 'EC',
    crv: 'P-256',
    x: 'yHTDYSfjU809fkSv9MmN4wuojf5c3cnD7ZDN13n-jz4',
    y: '8Mpkn744A5KDag0DmX2YivB63srjbugYZzWc3JOpQXI',
  }
  dependencies.set(tokens.HttpService, {
    rpcRequest: async options => {
      calls.push(options)
      return {
        body: {
          result:
            options.method === 'setup.get-pubkey'
              ? key
              : { guid: null, osDrive: null, attach: false, mokEnrolled: true },
        },
      }
    },
  })
  const client = new (load(
    'projects/start-os/web/setup-wizard/src/app/services/live-api.service.ts',
  ).LiveApiService)()
  await client.loadPubKey()
  assert.equal(calls[0].method, 'setup.get-pubkey')
  assert.equal(client.pubkey.kty, 'EC')
  const encrypted = await client.encrypt('secret')
  assert.equal(typeof encrypted.encrypted, 'object')
  assert.ok(encrypted.encrypted.ciphertext)
  const params = {
    osDrive: null,
    dataDrive: { stablePath: '/dev/example', wipe: true },
  }
  assert.equal(
    (await client.rpc.setup.installOs(params, { timeout: 5 * 60 * 1000 })).guid,
    null,
  )
  assert.equal(calls[1].method, 'setup.install-os')
  assert.equal(calls[1].params, params)
  assert.equal(calls[1].timeout, 5 * 60 * 1000)
})

test('Setup normalizes every CIFS path separator', () => {
  const { normalizeCifsPath } = harness().load(
    'projects/start-os/web/setup-wizard/src/app/utils/cifs.ts',
  )
  assert.equal(
    normalizeCifsPath(String.raw`share\nested\backups`),
    'share/nested/backups',
  )
})

test('Brochure serves each registry root through its own client', async () => {
  const { dependencies, tokens, load } = harness()
  let response = { id: 'registry', name: 'Registry' }
  const calls = []
  dependencies.set(tokens.HttpService, {
    rpcRequest: async (options, url) => {
      calls.push({ options, url })
      return { body: { result: response } }
    },
  })
  const Brochure = load(
    'projects/brochure-marketplace/src/app/services/live-api.service.ts',
  ).LiveApiService
  const client = Object.create(Brochure.prototype)
  client.http = dependencies.get(tokens.HttpService)
  client.registries = new Map()
  const registry = client.registry('https://registry.test/base/')
  assert.equal(await registry.info({}), response)
  assert.equal(calls[0].options.method, 'info')
  assert.equal(calls[0].url, 'https://registry.test/base/rpc/v0')
  assert.equal(client.registry('https://registry.test/base/'), registry)
  response = { categories: [], otherVersions: {} }
  const params = {
    id: 'example',
    targetVersion: null,
    sourceVersion: null,
    otherVersions: 'short',
  }
  assert.equal(await registry.package.get(params), response)
  assert.equal(calls[1].options.method, 'package.get')
  assert.equal(calls[1].options.params, params)
  assert.equal(calls[1].url, 'https://registry.test/base/rpc/v0')
})

test('StartOS mock registry calls reach list and single-package fixtures', async () => {
  const storage = Symbol('session storage')
  const pkg = { categories: [], best: {}, otherVersions: {} }
  const version = { ...pkg, best: { '1.0.0': {} } }
  const fixtures = {
    RegistryPackages: { example: pkg },
    OtherPackageVersions: { example: { '=1.0.0': version } },
  }
  const { dependencies, tokens, load } = harness(
    new Map([
      ['@ng-web-apis/common', { WA_SESSION_STORAGE: storage }],
      ['src/app/utils/acme', {}],
      ['./api.fixures', { Mock: fixtures }],
      ['./mock-patch', { mockPatchData: {} }],
      ['./md-sample.md', ''],
    ]),
  )
  dependencies.set(storage, null)
  dependencies.set(tokens.AuthService, { isVerified$: of(true) })
  dependencies.set(tokens.PATCH_CACHE, new BehaviorSubject({ value: {} }))
  const client = new (load(
    'projects/start-os/web/ui/src/app/services/api/embassy-mock-api.service.ts',
  ).MockApiService)()
  const params = {
    registry: 'https://registry.test/',
    id: null,
    targetVersion: null,
    sourceVersion: null,
    otherVersions: 'short',
  }
  assert.equal(
    await client.rpc.registry.package.get(params),
    fixtures.RegistryPackages,
  )
  assert.equal(
    await client.rpc.registry.package.get({ ...params, id: 'example' }),
    pkg,
  )
  assert.equal(
    await client.rpc.registry.package.get({
      ...params,
      id: 'example',
      targetVersion: '=1.0.0',
    }),
    version,
  )
})

test('StartOS DNS mock updates the selected gateway using live gateway state', async () => {
  const storage = Symbol('session storage')
  const { dependencies, tokens, load } = harness(
    new Map([
      ['@ng-web-apis/common', { WA_SESSION_STORAGE: storage }],
      ['src/app/utils/acme', {}],
      ['./api.fixures', { Mock: {} }],
      ['./mock-patch', { mockPatchData: {} }],
      ['./md-sample.md', ''],
    ]),
  )
  const gateway = { dns: {}, ipInfo: { dnsServers: ['10.64.0.1'] } }
  const cache = new BehaviorSubject({
    value: { serverInfo: { network: { gateways: { wg0: gateway } } } },
  })
  dependencies.set(storage, null)
  dependencies.set(tokens.AuthService, { isVerified$: of(true) })
  dependencies.set(tokens.PATCH_CACHE, cache)
  const client = new (load(
    'projects/start-os/web/ui/src/app/services/api/embassy-mock-api.service.ts',
  ).MockApiService)()
  const snapshot = await client.rpc.db.subscribe({})
  assert.ok(
    snapshot.dump.id > 0,
    'the snapshot must advance the empty client cache',
  )
  const patches = []
  const revisionIds = []
  const subscription = client.mockWsSource$.subscribe(revision => {
    patches.push(revision.patch)
    revisionIds.push(revision.id)
  })
  await client.rpc.net.dns.setStatic({
    gateway: 'wg0',
    servers: ['192.0.2.53', '2001:db8::53', '[2001:db8::54]:5353'],
  })
  assert.ok(
    revisionIds[0] > snapshot.dump.id,
    'the first mutation must follow the snapshot revision',
  )
  assert.deepEqual(JSON.parse(JSON.stringify(patches.pop())), [
    {
      op: 'replace',
      path: '/serverInfo/network/gateways/wg0/dns',
      value: {
        staticServers: [
          '192.0.2.53:53',
          '[2001:db8::53]:53',
          '[2001:db8::54]:5353',
        ],
        servers: ['192.0.2.53:53', '[2001:db8::53]:53', '[2001:db8::54]:5353'],
        source: 'static',
      },
    },
  ])
  await client.rpc.net.dns.setStatic({ gateway: 'wg0', servers: null })
  assert.deepEqual(JSON.parse(JSON.stringify(patches.pop()[0].value)), {
    staticServers: null,
    servers: ['10.64.0.1:53'],
    source: 'gateway',
  })
  cache.value.value.serverInfo.network.gateways.fresh = {
    dns: {},
    ipInfo: null,
  }
  await client.rpc.net.dns.setStatic({ gateway: 'fresh', servers: null })
  assert.deepEqual(JSON.parse(JSON.stringify(patches.pop()[0])), {
    op: 'replace',
    path: '/serverInfo/network/gateways/fresh/dns',
    value: {
      staticServers: null,
      servers: ['1.1.1.1:53', '[2606:4700:4700::1111]:53'],
      source: 'public-fallback',
    },
  })
  await assert.rejects(
    client.rpc.net.dns.setStatic({ gateway: 'wg0', servers: [] }),
    /At least one/,
  )
  await assert.rejects(
    client.rpc.net.dns.setStatic({ gateway: 'missing', servers: null }),
    /Unknown gateway/,
  )
  await client.rpc.net.gateway.edit({
    id: 'wg0',
    name: 'Renamed VPN',
    dns: { servers: ['192.0.2.55'] },
  })
  assert.deepEqual(JSON.parse(JSON.stringify(patches.pop())), [
    {
      op: 'replace',
      path: '/serverInfo/network/gateways/wg0/dns',
      value: {
        staticServers: ['192.0.2.55:53'],
        servers: ['192.0.2.55:53'],
        source: 'static',
      },
    },
    {
      op: 'replace',
      path: '/serverInfo/network/gateways/wg0/name',
      value: 'Renamed VPN',
    },
  ])
  const count = patches.length
  await assert.rejects(
    client.rpc.net.gateway.edit({
      id: 'wg0',
      name: 'Not saved',
      dns: { servers: [] },
    }),
    /At least one/,
  )
  assert.equal(patches.length, count)
  await client.rpc.net.gateway.setDefaultOutbound({
    gateway: 'wg0',
    dns: { servers: ['192.0.2.56'] },
  })
  assert.deepEqual(JSON.parse(JSON.stringify(patches.pop())), [
    {
      op: 'replace',
      path: '/serverInfo/network/defaultOutbound',
      value: 'wg0',
    },
    {
      op: 'replace',
      path: '/serverInfo/network/outboundDns',
      value: ['192.0.2.56:53'],
    },
  ])
  await client.rpc.package.setOutboundGateway({
    package: 'example',
    gateway: null,
    dns: { servers: null },
  })
  assert.deepEqual(JSON.parse(JSON.stringify(patches.pop())), [
    {
      op: 'replace',
      path: '/packageData/example/outboundGateway',
      value: null,
    },
    { op: 'replace', path: '/packageData/example/outboundDns', value: null },
  ])
  await client.rpc.net.gateway.setDefaultOutbound({ gateway: null })
  assert.equal(patches.pop().length, 1)
  subscription.unsubscribe()
})

test('StartOS list capacity respects exact and zero limits', () => {
  const arrayToken = Symbol('FormArrayName')
  const control = {
    controls: [],
    get length() {
      return this.controls.length
    },
  }
  const overrides = new Map(
    [
      '@angular/core/rxjs-interop',
      '@start9labs/shared',
      '@taiga-ui/cdk',
      '@taiga-ui/core',
      '@taiga-ui/kit',
      'src/app/services/form.service',
      '../pipes/hint.pipe',
      '../pipes/mustache.pipe',
      './control.component',
      './control.directive',
      './object.component',
    ].map(source => [source, {}]),
  )
  overrides.set('@angular/core', {
    Component: () => value => value,
    Input: () => () => {},
    forwardRef: factory => factory,
    inject: token => (token === arrayToken ? { control } : {}),
  })
  overrides.set('@angular/forms', { FormArrayName: arrayToken })
  const { FormArrayComponent } = harness(overrides).load(
    'projects/start-os/web/ui/src/app/routes/portal/components/form/containers/array.component.ts',
  )
  const component = new FormArrayComponent()
  for (const [disabled, maxLength, length, expected] of [
    [false, 3, 0, true],
    [false, 3, 2, true],
    [false, 3, 3, false],
    [false, 0, 0, false],
    [false, null, 5, true],
    ['Unavailable', 3, 2, false],
    ['Unavailable', null, 0, false],
  ]) {
    component.spec = { disabled, maxLength }
    control.controls = Array(length)
    assert.equal(
      component.canAdd,
      expected,
      JSON.stringify({ disabled, maxLength, length }),
    )
  }
})

function gatewayDnsHarness() {
  const keys = Object.fromEntries(
    ['task', 'i18n', 'form', 'api', 'confirm', 'dialog'].map(key => [
      key,
      Symbol(key),
    ]),
  )
  let context
  const closed = []
  const requests = []
  const modals = []
  const tasks = {
    run: async task => {
      await task()
      return true
    },
  }
  const overrides = new Map([
    ['@angular/forms', {}],
    ['@angular/router', {}],
    ['@angular/common', {}],
    ['src/app/routes/portal/components/form.component', {}],
    [
      'src/app/services/form-dialog.service',
      { FormDialogService: keys.dialog },
    ],
    ['src/app/services/title.service', {}],
    ['src/app/services/action.service', {}],
    ['src/app/services/standard-actions.service', {}],
    ['../components/action.component', {}],
    ['./table.component', {}],
    ['@start9labs/shared', { TaskService: keys.task, i18nPipe: keys.i18n }],
    ['@taiga-ui/cdk', { tuiMarkControlAsTouchedAndValidate: () => {} }],
    ['@taiga-ui/core', {}],
    ['@taiga-ui/kit', { TuiConfirmService: keys.confirm }],
    ['@taiga-ui/layout', {}],
    ['@taiga-ui/polymorpheus', { injectContext: () => context }],
    ['src/app/routes/portal/components/form/containers/group.component', {}],
    ['src/app/services/api/embassy-api.service', { ApiService: keys.api }],
    [
      'src/app/services/form.service',
      { FormService: keys.form, trim: () => {} },
    ],
    [
      'src/app/utils/configBuilderToSpec',
      { configBuilderToSpec: async () => ({}) },
    ],
    [
      '@angular/core/rxjs-interop',
      {
        toObservable: signal =>
          signal.stream$.pipe(filter(value => value !== undefined)),
        toSignal: stream => {
          let value
          stream.subscribe(next => {
            value = next
          })
          return () => value
        },
      },
    ],
  ])
  const { dependencies, load } = harness(overrides)
  const { PatchDB } = require('patch-db-client')
  const gateway = {
    name: 'LAN',
    dns: { staticServers: null, servers: ['192.0.2.1:53'], source: 'gateway' },
    ipInfo: { deviceType: 'ethernet', lanIp: ['192.0.2.1'] },
  }
  const network = new BehaviorSubject({
    defaultOutbound: null,
    outboundDns: null,
    host: { privateDomains: { 'server.home': ['eth0'] } },
    gateways: {
      eth0: gateway,
      wg0: {
        ...gateway,
        name: 'VPN',
        ipInfo: { deviceType: 'wireguard', lanIp: ['198.51.100.1'] },
      },
    },
  })
  const packages = new BehaviorSubject({})
  dependencies.set(PatchDB, {
    watch$: (key, ...path) =>
      key === 'packageData'
        ? packages.pipe(map(value => (path.length ? value[path[0]] : value)))
        : path.at(-1) === 'gateways'
          ? network.pipe(map(value => value.gateways))
          : network,
  })
  dependencies.set(keys.task, tasks)
  dependencies.set(keys.dialog, {
    open: (_component, options) => modals.push(options),
  })
  dependencies.set(keys.confirm, { markAsPristine: () => {} })
  dependencies.set(keys.i18n, { transform: value => value })
  dependencies.set(keys.form, {
    createForm: (_spec, value) => ({ value, invalid: false }),
  })
  dependencies.set(keys.api, {
    rpc: {
      net: {
        gateway: {
          edit: async params => requests.push(params),
          setDefaultOutbound: async params => requests.push(params),
        },
      },
      package: { setOutboundGateway: async params => requests.push(params) },
    },
  })
  return {
    network,
    packages,
    gateway,
    closed,
    requests,
    tasks,
    dependencies,
    keys,
    load,
    modals,
    create: id => {
      context = { data: id, completeWith: () => closed.push(id) }
      return new (load(
        'projects/start-os/web/ui/src/app/routes/portal/routes/system/routes/gateways/edit.ts',
      ).default)()
    },
  }
}

test('StartOS gateway editor keeps its target and unsaved name/DNS edits across link updates', async () => {
  const { network, gateway, create } = gatewayDnsHarness()
  const dialog = create('eth0')
  await new Promise(setImmediate)
  assert.equal(dialog.data().warn.length, 1)
  assert.match(dialog.data().warn[0], /LAN/)
  const edited = dialog.data().form
  edited.value.name = 'New LAN name'
  edited.value.dns = {
    selection: 'static',
    value: { servers: ['9.9.9.9'] },
  }
  network.next({
    ...network.value,
    gateways: {
      ...network.value.gateways,
      eth0: { ...gateway, ipInfo: { ...gateway.ipInfo, wanIp: '203.0.113.1' } },
    },
  })
  await new Promise(setImmediate)
  assert.equal(dialog.data().form, edited)
  assert.equal(dialog.data().form.value.name, 'New LAN name')
  assert.equal(dialog.data().form.value.dns.value.servers[0], '9.9.9.9')
  network.next({
    ...network.value,
    gateways: {
      ...network.value.gateways,
      eth0: {
        ...gateway,
        dns: { ...gateway.dns, staticServers: ['192.0.2.53:53'] },
      },
    },
  })
  await new Promise(setImmediate)
  assert.notEqual(dialog.data().form, edited)
  assert.equal(dialog.data().form.value.dns.value.servers[0], '192.0.2.53:53')
  const vpn = create('wg0')
  await new Promise(setImmediate)
  assert.equal(vpn.data().name, 'VPN')
  assert.equal(vpn.data().form.value.dns.selection, 'automatic')
  network.next({
    ...network.value,
    gateways: { wg0: network.value.gateways.wg0 },
  })
  await new Promise(setImmediate)
  assert.equal(dialog.data().gateway, undefined)
  assert.equal(dialog.context.data, 'eth0')
  assert.equal(vpn.context.data, 'wg0')
})

test('StartOS gateway editor saves name and DNS together and closes only after success', async () => {
  const fixture = gatewayDnsHarness()
  const dialog = fixture.create('wg0')
  await new Promise(setImmediate)
  const form = dialog.data().form
  form.value.name = 'Renamed VPN'
  form.value.dns = {
    selection: 'static',
    value: { servers: ['192.0.2.53:5353'] },
  }
  form.invalid = true
  await dialog.save(form)
  assert.equal(fixture.requests.length, 0)
  assert.equal(fixture.closed.length, 0)
  form.invalid = false
  const run = fixture.tasks.run
  fixture.tasks.run = async () => false
  await dialog.save(form)
  assert.equal(fixture.closed.length, 0)
  fixture.tasks.run = run
  await dialog.save(form)
  assert.deepEqual(JSON.parse(JSON.stringify(fixture.requests)), [
    { id: 'wg0', name: 'Renamed VPN', dns: { servers: ['192.0.2.53:5353'] } },
  ])
  assert.deepEqual(fixture.closed, ['wg0'])
  const automatic = fixture.create('wg0')
  await new Promise(setImmediate)
  await automatic.save(automatic.data().form)
  assert.deepEqual(JSON.parse(JSON.stringify(fixture.requests[1])), {
    id: 'wg0',
    name: null,
    dns: { servers: null },
  })
})

test('StartOS gateway editor stops editing on disconnect and resumes after reconnect', async () => {
  const fixture = gatewayDnsHarness()
  const gateway = {
    ...fixture.gateway,
    name: null,
    ipInfo: { ...fixture.gateway.ipInfo, name: 'Automatic LAN' },
  }
  fixture.network.next({
    ...fixture.network.value,
    gateways: { eth0: gateway },
  })
  const dialog = fixture.create('eth0')
  await new Promise(setImmediate)
  const form = dialog.data().form
  assert.equal(form.value.name, 'Automatic LAN')
  fixture.network.next({
    ...fixture.network.value,
    gateways: { eth0: { ...gateway, ipInfo: null } },
  })
  await new Promise(setImmediate)
  assert.equal(dialog.data().form, form)
  assert.equal(dialog.data().name, 'eth0')
  assert.equal(dialog.data().gateway, undefined)
  await dialog.save(form)
  assert.equal(fixture.requests.length, 0)
  assert.equal(fixture.closed.length, 0)
  fixture.network.next({
    ...fixture.network.value,
    gateways: { eth0: gateway },
  })
  await new Promise(setImmediate)
  assert.equal(dialog.data().form, form)
  assert.equal(dialog.data().gateway.ipInfo.name, 'Automatic LAN')
  await dialog.save(form)
  assert.equal(fixture.requests[0].name, null)
})

test('StartOS gateway editor rejects disconnected and bridge targets', async () => {
  for (const ipInfo of [null, { deviceType: 'bridge', lanIp: [] }]) {
    const fixture = gatewayDnsHarness()
    fixture.network.value.gateways.eth0.ipInfo = ipInfo
    const dialog = fixture.create('eth0')
    await new Promise(setImmediate)
    assert.equal(dialog.data().gateway, undefined)
    await dialog.save(dialog.data().form)
    assert.equal(fixture.requests.length, 0)
    assert.equal(fixture.closed.length, 0)
  }
})

test('StartOS gateway list excludes disconnected interfaces, bridges and loopback without changing saved selections', () => {
  const { PatchDB } = require('patch-db-client')
  const overrides = new Map([
    [
      '@angular/core/rxjs-interop',
      {
        toSignal: stream => {
          let value
          stream.subscribe(next => {
            value = next
          })
          return () => value
        },
      },
    ],
  ])
  const fixture = harness(overrides)
  overrides.set('@angular/core', {
    Injectable: () => value => value,
    computed: fn => fn,
    inject: token => fixture.dependencies.get(token),
  })
  const network = new BehaviorSubject({
    defaultOutbound: 'wg0',
    gateways: {
      eth0: {
        name: null,
        ipInfo: {
          name: 'LAN',
          deviceType: 'ethernet',
          subnets: ['192.0.2.2/24'],
          wanIp: null,
        },
      },
      wg0: { name: 'VPN', ipInfo: null },
      podman0: { name: null, ipInfo: null },
      'custom-bridge': {
        name: null,
        ipInfo: { deviceType: 'bridge', subnets: [] },
      },
      lo: {
        name: null,
        ipInfo: { deviceType: 'loopback', subnets: [] },
      },
      lxcbr0: {
        name: null,
        ipInfo: {
          name: 'Bridge',
          deviceType: 'bridge',
          subnets: [],
          wanIp: null,
        },
      },
    },
  })
  fixture.dependencies.set(PatchDB, { watch$: () => network })
  const gatewayModule = fixture.load(
    'projects/start-os/web/ui/src/app/services/gateway.service.ts',
  )
  const service = new gatewayModule.GatewayService()
  assert.equal(
    service
      .gateways()
      .map(g => g.id)
      .join(','),
    'eth0',
  )
  assert.equal(service.defaultOutboundGateway().name, 'VPN')
  const choices = gatewayModule.outboundGatewayOptions(
    {
      eth0: { name: 'Same name', ipInfo: { deviceType: 'ethernet' } },
      eth1: { name: 'Same name', ipInfo: { deviceType: 'ethernet' } },
    },
    null,
  )
  assert.equal(new Set(Object.values(choices)).size, 2)
  assert.equal(choices.eth0, 'Same name (eth0)')
  assert.equal(choices.eth1, 'Same name (eth1)')
  network.next({
    ...network.value,
    gateways: { ...network.value.gateways, eth0: { name: null, ipInfo: null } },
  })
  assert.equal(service.gateways().length, 0)
  assert.equal(service.defaultOutbound(), 'wg0')
  assert.equal(service.defaultOutboundGateway().name, 'VPN')
  network.next({
    ...network.value,
    gateways: {
      ...network.value.gateways,
      wg0: {
        name: 'VPN',
        dns: { staticServers: ['192.0.2.53:53'] },
        ipInfo: { deviceType: 'wireguard', subnets: [] },
      },
    },
  })
  assert.equal(
    service
      .gateways()
      .map(g => g.id)
      .join(','),
    'wg0',
  )
  assert.equal(service.gateways()[0].dns.staticServers[0], '192.0.2.53:53')
})

test('StartOS host outbound form ignores observed link changes and saves gateway/DNS together', async () => {
  const fixture = gatewayDnsHarness()
  const page = new (fixture.load(
    'projects/start-os/web/ui/src/app/routes/portal/routes/system/routes/outbound-traffic/index.ts',
  ).default)()
  await new Promise(setImmediate)
  const initial = page.outboundForm()
  assert.equal(initial.value.dns.selection, 'automatic')
  fixture.network.next({
    ...fixture.network.value,
    gateways: {
      ...fixture.network.value.gateways,
      eth0: {
        ...fixture.gateway,
        ipInfo: { ...fixture.gateway.ipInfo, wanIp: '203.0.113.42' },
      },
    },
  })
  await new Promise(setImmediate)
  assert.equal(page.outboundForm(), initial)
  fixture.network.next({
    ...fixture.network.value,
    outboundDns: ['192.0.2.55:53'],
  })
  await new Promise(setImmediate)
  assert.notEqual(page.outboundForm().key, initial.key)
  assert.equal(page.outboundForm().value.dns.value.servers[0], '192.0.2.55:53')
  await page.outboundButtons[0].handler({
    gateway: 'wg0',
    dns: { selection: 'static', value: { servers: ['192.0.2.56:5353'] } },
  })
  assert.deepEqual(JSON.parse(JSON.stringify(fixture.requests)), [
    { gateway: 'wg0', dns: { servers: ['192.0.2.56:5353'] } },
  ])
})

test('StartOS service outbound page keeps independent intent scoped to its route and lifecycle', async () => {
  const fixture = gatewayDnsHarness()
  fixture.network.value.outboundDns = ['192.0.2.55:53']
  const pkg = {
    stateInfo: { state: 'installed', manifest: { id: 'example' } },
    statusInfo: {
      desired: { main: 'stopped' },
      started: null,
      health: {},
      error: null,
    },
    tasks: {},
    currentDependencies: {},
    outboundGateway: null,
    outboundDns: null,
  }
  fixture.packages.next({
    example: pkg,
    other: {
      ...pkg,
      stateInfo: { state: 'installed', manifest: { id: 'other' } },
      outboundDns: ['192.0.2.58:53'],
    },
  })
  const page = new (fixture.load(
    'projects/start-os/web/ui/src/app/routes/portal/routes/services/routes/outbound-traffic/index.ts',
  ).default)()
  page.pkgId.set('example')
  await new Promise(setImmediate)
  const initial = page.outboundForm()
  assert.equal(initial.value.dns.selection, 'automatic')
  assert.equal(initial.inactive, false)
  fixture.network.next({
    ...fixture.network.value,
    outboundDns: ['192.0.2.99:53'],
  })
  await new Promise(setImmediate)
  assert.equal(page.outboundForm(), initial)
  await initial.buttons[0].handler({
    gateway: initial.value.gateway,
    dns: { selection: 'static', value: { servers: ['192.0.2.57'] } },
  })
  page.pkgId.set('other')
  await new Promise(setImmediate)
  const other = page.outboundForm()
  assert.notEqual(other.key, initial.key)
  assert.equal(other.value.dns.value.servers[0], '192.0.2.58:53')
  await other.buttons[0].handler(other.value)
  assert.deepEqual(JSON.parse(JSON.stringify(fixture.requests)), [
    { package: 'example', gateway: null, dns: { servers: ['192.0.2.57'] } },
    { package: 'other', gateway: null, dns: { servers: ['192.0.2.58:53'] } },
  ])
  fixture.packages.next({
    ...fixture.packages.value,
    other: {
      ...fixture.packages.value.other,
      stateInfo: { state: 'removing' },
    },
  })
  await new Promise(setImmediate)
  assert.equal(page.outboundForm().inactive, true)
  fixture.packages.next({ example: pkg })
  await new Promise(setImmediate)
  assert.equal(page.outboundForm(), null)
})

test('Brochure reports malformed registry URLs and can load the next selection', async () => {
  const source = harness()
  const shared = {
    ...source.load('shared-libs/ts-modules/shared/src/util/misc.util.ts'),
    ...source.load(
      'shared-libs/ts-modules/shared/src/types/workspace-config.ts',
    ),
    ...source.load('shared-libs/ts-modules/shared/src/types/rpc.types.ts'),
    ...source.load('shared-libs/ts-modules/shared/src/classes/rpc-error.ts'),
    Exver: Symbol('Exver'),
    i18nPipe: Symbol('i18nPipe'),
    i18nService: Symbol('i18nService'),
  }
  const storage = Symbol('local storage')
  const { dependencies, load } = harness(
    new Map([
      ['@ng-web-apis/common', { WA_LOCAL_STORAGE: storage }],
      ['@start9labs/shared', shared],
      [
        '@start9labs/marketplace',
        {
          ...source.load('shared-libs/ts-modules/marketplace/src/types.ts'),
          ...source.load(
            'shared-libs/ts-modules/marketplace/src/services/abstract-marketplace.service.ts',
          ),
        },
      ],
    ]),
  )
  const info = { name: 'Registry', categories: {}, icon: null }
  const Brochure = load(
    'projects/brochure-marketplace/src/app/services/live-api.service.ts',
  ).LiveApiService
  const api = Object.create(Brochure.prototype)
  api.registries = new Map()
  api.http = {
    rpcRequest: async ({ method }) => ({
      body: { result: method === 'info' ? info : {} },
    }),
  }
  dependencies.set(
    load('projects/brochure-marketplace/src/app/services/api.service.ts')
      .ApiService,
    api,
  )
  dependencies.set(storage, { getItem: () => null })
  dependencies.set(shared.Exver, {})
  dependencies.set(shared.i18nPipe, { transform: value => value })
  dependencies.set(shared.i18nService, { localize: value => value })
  const service = new (load(
    'projects/brochure-marketplace/src/app/services/marketplace.service.ts',
  ).MarketplaceService)()
  const failures = []
  const errors = []
  const registries = []
  const subscription = service.currentRegistry$.subscribe({
    next: registry => registries.push(registry),
    error: error => errors.push(error),
  })
  service.currentRegistryUrl$.next('not-a-url')
  const errorSubscription = service.registryError$.subscribe(url =>
    failures.push(url),
  )
  try {
    await new Promise(resolve => setImmediate(resolve))
    assert.deepEqual(errors, [])
    assert.deepEqual(failures, ['not-a-url'])
    assert.equal(subscription.closed, false)
    service.currentRegistryUrl$.next('https://registry.test/')
    await new Promise(resolve => setImmediate(resolve))
    assert.equal(registries.length, 1)
    assert.equal(registries[0].url, 'https://registry.test/')
    assert.equal(registries[0].info.name, info.name)
    assert.equal(registries[0].packages.length, 0)
  } finally {
    subscription.unsubscribe()
    errorSubscription.unsubscribe()
  }
})

function wrtMockOverrides() {
  return new Map([
    ['src/app/utils/workspace-config', { GIT_HASH: 'fixture' }],
    ['src/app/routes/lan/routes/ipv6/uci/mocks', {}],
    ['src/app/routes/outbound/utils', {}],
    ['src/app/routes/devices/uci/mocks', { mockDhcpHosts: [] }],
  ])
}

test('WRT mock mutations resolve nullable partial profile queries into full LAN references', async () => {
  const { load } = harness(wrtMockOverrides())
  const client = Object.create(
    load('projects/start-wrt/web/src/app/services/api/mock-api.service.ts')
      .MockApiService.prototype,
  )
  const existing = { fullname: 'Existing', interface: 'abcde', vlan_tag: 101 }
  client.mockProfiles = [existing]
  client.logActivity = () => {}
  const params = {
    fullname: 'Created',
    gateway_ip: '192.168.50.1',
    outbound: 'wan',
    lan_access: { other_profiles: [{ fullname: 'Existing', vlan_tag: null }] },
    wan_access: { blacklist: [] },
    access_to_new_profiles: false,
    owns_lan: false,
  }
  const id = await client.profileCreate(params)
  assert.deepEqual(
    JSON.parse(
      JSON.stringify(client.mockProfiles[1].lan_access.other_profiles),
    ),
    [existing],
  )
  await client.profileUpdate({
    ...params,
    ...id,
    lan_access: { other_profiles: [{ interface: existing.interface }] },
  })
  assert.deepEqual(
    JSON.parse(
      JSON.stringify(client.mockProfiles[1].lan_access.other_profiles),
    ),
    [existing],
  )
  await assert.rejects(
    client.profileCreate({
      ...params,
      lan_access: { other_profiles: [{ fullname: 'Missing' }] },
    }),
    /Profile not found/,
  )
})

test('WRT recognises VPN peer rejections through shared RPC errors', () => {
  const { load } = harness()
  const { RpcError } = load(
    'shared-libs/ts-modules/shared/src/classes/rpc-error.ts',
  )
  const { isVpnPeersWouldBreak } = load(
    'projects/start-wrt/web/src/app/services/vpn-peers.ts',
  )
  assert.equal(
    isVpnPeersWouldBreak(
      new RpcError({
        code: 1019,
        message: 'VPN Peers Would Break',
        data: { details: 'VPN peers would break: 1 peer(s)' },
      }),
    ),
    true,
  )
  for (const error of [
    null,
    'VPN Peers Would Break',
    { code: 0 },
    new RpcError({
      code: -1,
      message: 'General error',
      data: 'VPN Peers Would Break: VPN peers would break: 1 peer(s)',
    }),
    new Error('VPN client failed'),
  ]) {
    assert.equal(isVpnPeersWouldBreak(error), false)
  }
})

for (const method of ['lanIpv4Set', 'profileUpdate']) {
  test(`WRT ${method} guards VPN peers across profiles on a Network Block change`, async () => {
    const { load } = harness(wrtMockOverrides())
    const { MockApiService } = load(
      'projects/start-wrt/web/src/app/services/api/mock-api.service.ts',
    )
    const { isVpnPeersWouldBreak } = load(
      'projects/start-wrt/web/src/app/services/vpn-peers.ts',
    )
    const client = Object.create(MockApiService.prototype)
    const admin = {
      fullname: 'Admin',
      interface: 'lan',
      vlan_tag: 1,
      owns_lan: true,
      gateway_ip: '192.168.1.1',
      lan_access: 'ALL',
      wan_access: 'ALL',
      outbound: 'wan',
      access_to_new_profiles: true,
    }
    const guest = {
      ...admin,
      fullname: 'Guest',
      interface: 'guest',
      vlan_tag: 100,
      owns_lan: false,
      gateway_ip: '192.168.2.1',
    }
    client.mockProfiles = [admin, guest]
    client.mockLanIpv4 = { address: admin.gateway_ip }
    client.mockVpnServers = [
      { profile: 'guest', peers: [{ name: 'phone' }] },
      { profile: 'lan', peers: [] },
    ]
    client.logActivity = () => {}
    let persisted = 0
    client.persistVpnServers = () => persisted++
    const save = (address, force) =>
      client[method](
        method === 'lanIpv4Set'
          ? { address, force }
          : { ...admin, gateway_ip: address, force },
      )

    await save(admin.gateway_ip)
    await save('192.168.4.1')
    assert.equal(client.mockVpnServers.length, 2)
    await assert.rejects(save('10.10.4.1'), isVpnPeersWouldBreak)
    assert.equal(client.mockLanIpv4.address, '192.168.4.1')
    assert.equal(client.mockVpnServers.length, 2)
    assert.equal(persisted, 0)
    await save('10.10.4.1', true)
    assert.equal(client.mockLanIpv4.address, '10.10.4.1')
    assert.deepEqual(
      Array.from(client.mockVpnServers, s => s.profile),
      ['lan'],
    )
    assert.equal(persisted, 1)
  })
}

test('WRT timeout unsubscribes from HTTP and retains code-zero reconnect errors', async () => {
  const { dependencies, tokens, load } = harness()
  let aborted = 0
  let url
  dependencies.set(tokens.StaleUiService, {
    checkHash() {
      assert.fail('Timed-out request must not check firmware hash')
    },
  })
  dependencies.set(tokens.HttpClient, {
    post: target => {
      url = target
      return new Observable(() => () => aborted++)
    },
  })
  const client = new (load(
    'projects/start-wrt/web/src/app/services/http.service.ts',
  ).HttpService)()
  await assert.rejects(
    client.request({ body: { method: 'auth.login', params: {} }, timeout: 10 }),
    error => error.code === 0 && error.message === 'Network timeout',
  )
  assert.equal(aborted, 1)
  assert.equal(url, 'https://router.test/rpc/v1')
})

test('WRT checks firmware hashes and preserves HTTP error details', async () => {
  const { dependencies, tokens, load } = harness()
  const hashes = []
  let fail = false
  dependencies.set(tokens.StaleUiService, {
    checkHash: hash => hashes.push(hash),
  })
  const response = {
    body: { result: null },
    headers: { get: () => 'firmware' },
  }
  dependencies.set(tokens.HttpClient, {
    post: () =>
      fail
        ? throwError(() => ({ status: 503, statusText: 'Unavailable' }))
        : of(response),
  })
  const client = new (load(
    'projects/start-wrt/web/src/app/services/http.service.ts',
  ).HttpService)()
  assert.equal(await client.request({ body: {} }), response)
  assert.deepEqual(hashes, ['firmware'])
  fail = true
  await assert.rejects(
    client.request({ body: {} }),
    error => error.code === 503 && error.message === 'Unavailable',
  )
})

test('WRT lazily invalidates authentication and preserves host errors and nullable results', async () => {
  const { dependencies, tokens, load } = harness()
  let invalidated = 0
  let lookups = 0
  let response = { result: null }
  dependencies.set(tokens.AuthKeyService, { signRpcHeaders: async () => ({}) })
  dependencies.set(tokens.HttpService, {
    request: async () => ({ body: response }),
  })
  dependencies.set(tokens.Injector, {
    get: token => {
      assert.equal(token, tokens.AuthService)
      lookups++
      return { setUnverified: () => invalidated++ }
    },
  })
  const client = new (load(
    'projects/start-wrt/web/src/app/services/rpc.service.ts',
  ).RpcService)()
  assert.equal(
    await client.request({ method: 'auth.logout', params: {} }),
    null,
  )
  assert.equal(lookups, 0)
  response = { error: { code: 34, message: 'unauthorized', data: 'expired' } }
  await assert.rejects(
    client.request({ method: 'auth.logout', params: {} }),
    error =>
      error.code === 34 &&
      error.message === 'RPC ERROR: unauthorized\n\nexpired',
  )
  assert.equal(invalidated, 1)
  response = { error: { code: 12, message: 'failed', data: { details: 42 } } }
  await assert.rejects(
    client.request({ method: 'auth.logout', params: {} }),
    error => error.code === 12 && error.message === 'RPC ERROR: failed',
  )
  assert.equal(lookups, 1)
  response = null
  await assert.rejects(
    client.request({ method: 'auth.logout', params: {} }),
    /Empty RPC response/,
  )
})

function startupStateHarness() {
  const rxjs = require('rxjs')
  const ticks = new rxjs.Subject()
  const keys = Object.fromEntries(
    ['api', 'router', 'network', 'alerts', 'i18n'].map(key => [
      key,
      Symbol(key),
    ]),
  )
  const { dependencies, load } = harness(
    new Map([
      ['rxjs', { ...rxjs, timer: () => ticks }],
      ['@angular/router', { Router: keys.router }],
      ['@start9labs/shared', { i18nPipe: keys.i18n }],
      ['@taiga-ui/cdk', {}],
      ['@taiga-ui/core', { TuiNotificationService: keys.alerts }],
      ['@taiga-ui/polymorpheus', { PolymorpheusComponent: class {} }],
      ['src/app/services/api/embassy-api.service', { ApiService: keys.api }],
      ['src/app/services/network.service', { NetworkService: keys.network }],
    ]),
  )
  const routes = []
  dependencies.set(keys.router, {
    navigate: route => routes.push(route[0]),
    isActive: () => true,
  })
  dependencies.set(keys.network, new BehaviorSubject(true))
  dependencies.set(keys.alerts, { open: () => rxjs.NEVER })
  dependencies.set(keys.i18n, { transform: value => value })
  return {
    ticks,
    routes,
    create: rpc => {
      dependencies.set(keys.api, { rpc })
      return new (load(
        'projects/start-os/web/ui/src/app/services/state.service.ts',
      ).StateService)()
    },
  }
}

test('StartOS refreshes startup DNS without rerouting and stops polling at runtime', async () => {
  const { ticks, routes, create } = startupStateHarness()
  let state = 'initializing'
  let status = { bootstrap: true, gateway: null }
  let statusFailure = false
  let stateCalls = 0
  let dnsCalls = 0
  const service = create({
    state: async () => {
      stateCalls++
      return state
    },
    net: {
      dns: {
        status: async () => {
          dnsCalls++
          if (statusFailure) throw new Error('unavailable')
          return status
        },
      },
    },
  })
  const subscription = service.subscribe()
  try {
    ticks.next()
    await new Promise(setImmediate)
    assert.equal(service.dnsStatus(), status)
    assert.deepEqual(routes, ['initializing'])
    status = { bootstrap: false, gateway: null }
    ticks.next()
    await new Promise(setImmediate)
    assert.equal(service.dnsStatus(), status)
    assert.deepEqual(routes, ['initializing'])
    state = 'error'
    ticks.next()
    await new Promise(setImmediate)
    assert.deepEqual(routes, ['initializing', 'diagnostic'])
    statusFailure = true
    ticks.next()
    await new Promise(setImmediate)
    assert.equal(service.dnsStatus(), null)
    assert.deepEqual(routes, ['initializing', 'diagnostic'])
    state = 'running'
    ticks.next()
    await new Promise(setImmediate)
    assert.equal(service.dnsStatus(), null)
    assert.deepEqual(routes, ['initializing', 'diagnostic', ''])
    assert.equal(dnsCalls, 4)
    ticks.next()
    await new Promise(setImmediate)
    assert.equal(stateCalls, 5)
  } finally {
    subscription.unsubscribe()
  }
})

test('StartOS ignores a superseded startup DNS request', async () => {
  const { ticks, create } = startupStateHarness()
  let finishOldRequest
  const oldRequest = new Promise(resolve => (finishOldRequest = resolve))
  const current = { bootstrap: false, gateway: null }
  let calls = 0
  const service = create({
    state: async () => 'initializing',
    net: {
      dns: { status: async () => (++calls === 1 ? oldRequest : current) },
    },
  })
  const subscription = service.subscribe()
  try {
    ticks.next()
    await new Promise(setImmediate)
    ticks.next()
    await new Promise(setImmediate)
    assert.equal(calls, 1)
    service.trigger$.next(true)
    ticks.next()
    await new Promise(setImmediate)
    assert.equal(service.dnsStatus(), current)
    finishOldRequest({ bootstrap: true, gateway: null })
    await new Promise(setImmediate)
    assert.equal(service.dnsStatus(), current)
  } finally {
    subscription.unsubscribe()
  }
})
