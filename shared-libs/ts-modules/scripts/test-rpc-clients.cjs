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

test('StartOS confines database projection to the root and preserves action protocol values', async () => {
  const { load } = harness()
  const client = Object.create(
    load(
      'projects/start-os/web/ui/src/app/services/api/embassy-live-api.service.ts',
    ).LiveApiService.prototype,
  )
  const value = { serverInfo: { id: 'server' }, packageData: {} }
  const dump = { guid: 'subscription', dump: { id: 3, value } }
  const spec = { type: 'object', fields: {}, fieldOrder: [] }
  const input = { input: null, spec, eventId: 'input-event' }
  const action = { version: '1', title: 'Done', message: null, result: null }
  const requests = []
  let response = dump
  client.rpcRequest = async options => {
    requests.push(options)
    return response
  }
  const subscribed = await client.subscribeToPatchDB()
  assert.equal(subscribed.guid, dump.guid)
  assert.equal(subscribed.dump.id, dump.dump.id)
  assert.equal(subscribed.dump.value, value)
  assert.equal(requests[0].method, 'db.subscribe')
  assert.equal(Object.keys(requests[0].params).length, 0)
  response = input
  const received = await client.getActionInput({ id: 'demo', actionId: 'test' })
  assert.equal(received.spec, spec)
  assert.equal(received.eventId, input.eventId)
  response = null
  assert.equal(
    await client.getActionInput({ id: 'demo', actionId: 'test' }),
    null,
  )
  response = action
  assert.equal(
    await client.runAction({ id: 'demo', actionId: 'test', input: null }),
    action,
  )
  response = null
  assert.equal(
    await client.runAction({ id: 'demo', actionId: 'test', input: null }),
    null,
  )
})

test('Setup converts the actual JWK response and preserves installation timeout', async () => {
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
  await client.getPubKey()
  assert.equal(client.pubkey.kty, 'EC')
  const encrypted = await client.encrypt('secret')
  assert.equal(typeof encrypted.encrypted, 'object')
  assert.ok(encrypted.encrypted.ciphertext)
  const params = {
    osDrive: null,
    dataDrive: { stablePath: '/dev/example', wipe: true },
  }
  assert.equal((await client.installOs(params)).guid, null)
  assert.equal(calls[1].params, params)
  assert.equal(calls[1].timeout, 5 * 60 * 1000)
})

test('Setup normalizes every CIFS path separator before verification and restore', async () => {
  const { dependencies, tokens, load } = harness()
  const calls = []
  dependencies.set(tokens.HttpService, {
    rpcRequest: async options => {
      calls.push(options)
      return { body: { result: {} } }
    },
  })
  const client = new (load(
    'projects/start-os/web/setup-wizard/src/app/services/live-api.service.ts',
  ).LiveApiService)()
  const cifs = {
    hostname: 'nas',
    path: String.raw`share\nested\backups`,
    username: 'user',
    password: null,
  }
  await client.verifyCifs({ ...cifs })
  assert.equal(calls[0].params.path, 'share/nested/backups')
  await client.execute({
    guid: 'data',
    kiosk: false,
    recoverySource: {
      type: 'backup',
      target: { type: 'cifs', ...cifs },
      serverId: 'server',
      password: { encrypted: {} },
    },
  })
  assert.equal(
    calls[1].params.recoverySource.target.path,
    'share/nested/backups',
  )
})

test('Brochure uses the direct registry root and rejects mismatched package projections', async () => {
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
  assert.equal(
    await client.getRegistryInfo('https://registry.test/base/'),
    response,
  )
  assert.equal(calls[0].options.method, 'info')
  assert.equal(calls[0].url, 'https://registry.test/base/rpc/v0')
  response = { categories: [], otherVersions: {} }
  const single = await client.getRegistryPackage(
    'https://registry.test/',
    'example',
    null,
  )
  assert.deepEqual(single.categories, [])
  assert.equal(calls[1].options.params.otherVersions, 'short')
  assert.equal(calls[1].options.params.id, 'example')
  response = { example: response }
  assert.ok(
    (await client.getRegistryPackages('https://registry.test/')).example,
  )
  await assert.rejects(
    client.getRegistryPackage('https://registry.test/', 'example', null),
    /single package/,
  )
  response = { categories: [], otherVersions: {} }
  await assert.rejects(
    client.getRegistryPackages('https://registry.test/'),
    /package map/,
  )
  response = { categories: [] }
  await assert.rejects(
    client.getRegistryPackage('https://registry.test/', 'example', null),
    /Invalid package response/,
  )
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
