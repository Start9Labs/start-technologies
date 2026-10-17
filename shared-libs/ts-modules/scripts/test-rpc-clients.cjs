const assert = require('node:assert/strict')
const fs = require('node:fs')
const path = require('node:path')
const test = require('node:test')
const vm = require('node:vm')
const ts = require('typescript')
const { BehaviorSubject, Observable, of, throwError } = require('rxjs')

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

test('WRT mock mutations resolve nullable partial profile queries into full LAN references', async () => {
  const { load } = harness(
    new Map([
      ['src/app/utils/workspace-config', { GIT_HASH: 'fixture' }],
      ['src/app/routes/lan/routes/ipv6/uci/mocks', {}],
      ['src/app/routes/outbound/utils', {}],
      ['src/app/routes/devices/uci/mocks', { mockDhcpHosts: [] }],
    ]),
  )
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
  const details = 'VPN Peers Would Break: VPN peers would break: 1 peer(s)'
  for (const data of [details, { details }]) {
    assert.equal(
      isVpnPeersWouldBreak(
        new RpcError({ code: -1, message: 'General error', data }),
      ),
      true,
    )
  }
  assert.equal(isVpnPeersWouldBreak({ code: 1019 }), true)
  for (const error of [
    null,
    'VPN Peers Would Break',
    { code: 0 },
    new Error('VPN client failed'),
  ]) {
    assert.equal(isVpnPeersWouldBreak(error), false)
  }
})

for (const method of ['lanIpv4Set', 'profileUpdate']) {
  test(`WRT ${method} guards VPN peers across profiles on a Network Block change`, async () => {
    const { load } = harness(
      new Map([
        ['src/app/utils/workspace-config', { GIT_HASH: 'fixture' }],
        ['src/app/routes/lan/routes/ipv6/uci/mocks', {}],
        ['src/app/routes/outbound/utils', {}],
        ['src/app/routes/devices/uci/mocks', { mockDhcpHosts: [] }],
      ]),
    )
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
  let credentials
  dependencies.set(tokens.StaleUiService, {
    checkHash() {
      assert.fail('Timed-out request must not check firmware hash')
    },
  })
  dependencies.set(tokens.HttpClient, {
    post: (target, body, options) => {
      url = target
      credentials = options.withCredentials
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
  assert.equal(credentials, true)
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
