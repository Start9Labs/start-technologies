import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { DOCUMENT, Injector } from '@angular/core'
import { HttpHeaders } from '@angular/common/http'
import { BehaviorSubject, Subject } from 'rxjs'
import { loadTypeScript } from './typescript-loader.mjs'

const shared = new URL(
  '../../../../../shared-libs/ts-modules/shared/src/',
  import.meta.url,
)
loadTypeScript(
  new Map([
    [
      '@start9labs/shared',
      `
    export { AuthKeyService } from '${new URL('services/auth-key.service.ts', shared)}';
    export { HttpService } from '${new URL('services/http.service.ts', shared)}';
    export { RpcSocketService } from '${new URL('services/rpc-socket.service.ts', shared)}';
    export { ServerIdentityService } from '${new URL('services/server-identity.service.ts', shared)}';
    export { RELATIVE_URL } from '${new URL('tokens/relative-url.ts', shared)}';
    export { RpcError } from '${new URL('classes/rpc-error.ts', shared)}';
    export { isRpcError } from '${new URL('types/rpc.types.ts', shared)}';
    export { rpcRequestBody, rpcWebSocketUrl } from '${new URL('services/rpc-request.ts', shared)}';
    export { rpcResponseResult, waitForPatchSequence } from '${new URL('services/rpc-response.ts', shared)}';
    export { authenticatedRpcTransport, localRpcRequest } from '${new URL('services/local-rpc.ts', shared)}';
    export { databaseSubscription, legacyDatabaseSubscription, bufferDatabaseRevisions } from '${new URL('services/database-subscription.ts', shared)}';
  `,
    ],
    [
      'rxjs/webSocket',
      `
    export const opened = [];
    export const webSocket = config => { const socket = { config }; opened.push(socket); return socket; };
  `,
    ],
    ['../auth.service', 'export class AuthService {}'],
    [
      'src/app/services/patch-db/patch-db-source',
      'export const PATCH_CACHE = Symbol()',
    ],
  ]),
)
const {
  AuthKeyService,
  HttpService,
  RpcSocketService,
  RELATIVE_URL,
  ServerIdentityService,
} = await import('@start9labs/shared')
const { AuthService } = await import('../auth.service')
const { PATCH_CACHE } =
  await import('src/app/services/patch-db/patch-db-source')
const { ApiService } =
  await import('../src/app/services/api/embassy-api.service.ts')
const { LiveApiService } =
  await import('../src/app/services/api/embassy-live-api.service.ts')
const { StartupApiService, StartupLiveApiService } =
  await import('../src/app/services/api/startup-api.service.ts')

function fixture(context) {
  const calls = []
  const cache = new BehaviorSubject({ id: 0, value: {} })
  const lease = { key: {}, signal: new AbortController().signal }
  let rejected = 0
  const http = {
    rpcRequest: async options => {
      calls.push({ transport: 'startup', options })
      return { body: { result: 'initializing' } }
    },
    rpcMessage: async (body, options, url) => {
      calls.push({ transport: 'http', body, options, url })
      return { body: { result: 'public' } }
    },
    httpRequest: async options => {
      calls.push({ transport: 'resource', options })
      return { body: 'resource', headers: new HttpHeaders() }
    },
  }
  const sockets = {
    message: async (body, options, identity) => {
      calls.push({ transport: 'encrypted', body, options, identity })
      options.openObserver?.next({ peer: 'verified peer' })
      return {
        result:
          JSON.parse(body).method === 'state' ? 'initializing' : 'encrypted',
      }
    },
    stream: async (body, identity, options) => {
      calls.push({ transport: 'encrypted-stream', body, identity, options })
      return {
        initial: { target: null, progress: null },
        items: (async function* () {
          return null
        })(),
      }
    },
    open: (guid, config) => {
      const socket = new Subject()
      calls.push({ transport: 'encrypted-guid', guid, config, socket })
      return socket
    },
  }
  const keys = {
    admissionLease: async () => {
      calls.push({ transport: 'admission' })
      return lease
    },
    lease: async () => {
      calls.push({ transport: 'key' })
      return lease
    },
    signHeader: async (body, headers) => {
      calls.push({ transport: 'header', body, headers })
      return new HttpHeaders({
        ...headers,
        'X-Start-Auth-Sig': 'resource proof',
      })
    },
  }
  const observed = []
  const identity = {
    openObserver: { next: event => observed.push(event.peer) },
  }
  const injector = Injector.create({
    providers: [
      { provide: ApiService, useClass: LiveApiService },
      { provide: StartupApiService, useClass: StartupLiveApiService },
      {
        provide: DOCUMENT,
        useValue: { location: { origin: 'http://os.test' }, defaultView: {} },
      },
      { provide: RELATIVE_URL, useValue: '/rpc/v1' },
      { provide: HttpService, useValue: http },
      { provide: RpcSocketService, useValue: sockets },
      { provide: ServerIdentityService, useValue: identity },
      { provide: AuthKeyService, useValue: keys },
      { provide: AuthService, useValue: { setUnverified: () => rejected++ } },
      { provide: PATCH_CACHE, useValue: cache },
    ],
  })
  context.after(() => injector.destroy())
  return {
    api: injector.get(ApiService),
    startup: injector.get(StartupApiService),
    http,
    sockets,
    keys,
    calls,
    cache,
    lease,
    rejected: () => rejected,
    observed,
  }
}
const flush = () => new Promise(setImmediate)

test('startup typed updates capture input and cancel pending acquisition without peer-history writes', async context => {
  const f = fixture(context)
  let acquire
  f.keys.admissionLease = () => new Promise(resolve => (acquire = resolve))
  const params = { progress: true, targetVersion: '0.4.2.1' }
  const pending = f.startup.streams.diagnostic.update(params, { timeout: 1200 })
  params.targetVersion = '0.4.2.2'
  acquire(f.lease)
  const response = await pending
  assert.equal(response.initial.target, null)
  const call = f.calls.find(call => call.transport === 'encrypted-stream')
  assert.equal(JSON.parse(call.body).params.targetVersion, '0.4.2.1')
  assert.equal(call.options.timeout, 1200)
  assert.equal(call.options.openObserver, undefined)
  assert.equal(call.identity, f.lease)
  assert.deepEqual(f.observed, [])
  const abort = new AbortController()
  const canceled = f.startup.streams.diagnostic.update(
    {},
    { signal: abort.signal },
  )
  abort.abort()
  acquire(f.lease)
  await assert.rejects(canceled, { name: 'AbortError' })
  assert.equal(
    f.calls.filter(call => call.transport === 'encrypted-stream').length,
    1,
  )
})

test('startup probes use encrypted possession without enrolled authorization or advisory history', async context => {
  const f = fixture(context)
  f.keys.lease = () => {
    throw new Error('private identity accessed')
  }
  assert.equal(
    await f.startup.rpc.state({}, { timeout: 10000 }),
    'initializing',
  )
  assert.deepEqual(
    f.calls.map(call => call.transport),
    ['admission', 'encrypted'],
  )
  assert.deepEqual(JSON.parse(f.calls[1].body), { method: 'state', params: {} })
  assert.equal(f.calls[1].options.timeout, 10000)
  assert.deepEqual(f.observed, [])
})

test('an untrusted startup-mode reply cannot downgrade a private login', async context => {
  const f = fixture(context)
  assert.equal(await f.startup.rpc.state({}), 'initializing')
  assert.equal(
    await f.api.rpc.auth.login({ password: 'fixture', pubkey: 'public' }),
    'encrypted',
  )
  const message = f.calls.find(
    call =>
      call.transport === 'encrypted' &&
      JSON.parse(call.body).method === 'auth.login',
  )
  assert.equal(message.identity, f.lease)
  assert.deepEqual(JSON.parse(message.body), {
    method: 'auth.login',
    params: { password: 'fixture', pubkey: 'public' },
  })
  assert.equal(
    f.calls.some(call => call.transport === 'http'),
    false,
  )
})

for (const method of ['init.subscribe', 'diagnostic.error', 'net.dns.status'])
  test(`${method} uses the explicit startup client without credentials`, async context => {
    const f = fixture(context)
    f.keys.lease = () => {
      throw new Error('private identity accessed')
    }
    const [namespace, ...path] = method.split('.')
    let call = f.startup.rpc[namespace]
    for (const segment of path) call = call[segment]
    await call({})
    assert.deepEqual(
      f.calls.map(call => call.transport),
      ['admission', 'encrypted'],
    )
    assert.equal(JSON.parse(f.calls[1].body).method, method)
    assert.deepEqual(f.observed, [])
  })

test('startup GUIDs use shared encrypted admission and preserve callbacks and binary options', context => {
  const f = fixture(context)
  const config = {
    binaryType: 'arraybuffer',
    deserializer: event => event.data,
    serializer: value => value,
  }
  const socket = f.startup.openWebsocket$('startup-guid', config)
  const call = f.calls.at(-1)
  assert.equal(call.transport, 'encrypted-guid')
  assert.equal(call.guid, 'startup-guid')
  assert.equal(call.config, config)
  assert.equal(socket, call.socket)
  assert.equal(call.config.openObserver, undefined)
  assert.deepEqual(f.observed, [])
})

test('normal GUIDs always use fresh encrypted admission regardless of startup state', async context => {
  const f = fixture(context)
  await f.startup.rpc.state({})
  const config = {
    binaryType: 'arraybuffer',
    deserializer: event => event.data,
  }
  const socket = f.api.openWebsocket$('secure-guid', config)
  const call = f.calls.at(-1)
  assert.equal(call.transport, 'encrypted-guid')
  assert.equal(call.config.deserializer, config.deserializer)
  assert.equal(call.config.binaryType, config.binaryType)
  assert.equal(socket, call.socket)
  call.config.openObserver.next({ peer: 'verified normal peer' })
  assert.deepEqual(f.observed, ['verified normal peer'])
})

test('normal public calls without a device key retain anonymous HTTP', async context => {
  const f = fixture(context)
  f.keys.lease = async () => null
  assert.equal(await f.api.rpc.echo({ message: 'public' }), 'public')
  assert.equal(f.calls.length, 1)
  assert.equal(f.calls[0].transport, 'http')
})

test('failed encrypted admission never falls back to HTTP', async context => {
  const f = fixture(context)
  const failure = new Error('exchange rejected')
  f.sockets.message = async () => {
    throw failure
  }
  await assert.rejects(
    f.api.rpc.echo({ message: 'secret' }),
    error => error === failure,
  )
  assert.equal(
    f.calls.some(
      call => call.transport === 'http' || call.transport === 'startup',
    ),
    false,
  )
})

test('foreign echoes stay unsigned and ignore the local cache sequence', async context => {
  const f = fixture(context)
  f.keys.lease = () => {
    throw new Error('private identity accessed')
  }
  f.http.rpcMessage = async (body, options, url) => {
    assert.equal(url, 'https://foreign.test/rpc')
    assert.deepEqual(JSON.parse(body), {
      method: 'echo',
      params: { message: 'ping' },
    })
    return { body: { result: 'ping', meta: { patchSequence: 999 } } }
  }
  assert.equal(
    await f.api.echo({ message: 'ping' }, 'https://foreign.test/rpc'),
    'ping',
  )
  assert.deepEqual(f.calls, [])
  assert.equal(f.cache.observed, false)
})

test('startup captures request bytes before possession-key lookup and never records startup peers', async context => {
  const f = fixture(context)
  const ready = Promise.withResolvers()
  f.keys.admissionLease = () => ready.promise
  const params = { url: 'https://registry.test', version: '0.4.3' }
  const pending = f.startup.rpc.diagnostic.update(params)
  params.url = 'https://changed.test'
  ready.resolve(f.lease)
  await pending
  assert.equal(
    JSON.parse(f.calls.at(-1).body).params.url,
    'https://registry.test',
  )
  assert.deepEqual(f.observed, [])
})

test('canceled key lookup never falls back to HTTP or starts a late startup operation', async context => {
  const f = fixture(context)
  const canceled = new DOMException('disposed', 'AbortError')
  f.keys.admissionLease = async () => {
    throw canceled
  }
  await assert.rejects(f.startup.rpc.state({}), error => error === canceled)
  f.keys.lease = async () => {
    throw canceled
  }
  await assert.rejects(
    f.api.rpc.auth.login({ password: 'secret', pubkey: 'key' }),
    error => error === canceled,
  )
  assert.deepEqual(f.calls, [])
})

test('failed startup admission never selects plaintext transport', async context => {
  const f = fixture(context)
  f.sockets.message = async () => {
    throw Error('admission rejected')
  }
  await assert.rejects(f.startup.rpc.state({}), /admission rejected/)
  assert.equal(
    f.calls.some(
      call => call.transport === 'http' || call.transport === 'startup',
    ),
    false,
  )
  assert.deepEqual(f.observed, [])
})

test('RPC bytes are captured before waiting for the device identity', async context => {
  const f = fixture(context)
  let identity
  f.keys.lease = () =>
    new Promise(resolve => {
      identity = resolve
    })
  const params = { message: 'original' }
  const pending = f.api.rpc.echo(params)
  params.message = 'changed'
  identity(f.lease)
  assert.equal(await pending, 'encrypted')
  const sent = f.calls.find(call => call.transport === 'encrypted')
  assert.equal(JSON.parse(sent.body).params.message, 'original')
})

test('database streams emit their initiating dump without awaiting its cache fence and preserve authorization errors', async context => {
  const f = fixture(context)
  const messages = new Subject()
  const received = []
  let failure
  const { rpcResponseStream } = await import(
    new URL('services/rpc-stream.ts', shared)
  )
  f.sockets.stream = (body, identity, options) => {
    assert.deepEqual(JSON.parse(body), { method: 'db.subscribe', params: {} })
    assert.equal(identity, f.lease)
    return rpcResponseStream(messages, options)
  }
  const subscription = f.api.watchDb$().subscribe({
    next: value => received.push(value),
    error: error => {
      failure = error
    },
  })
  await flush()
  const dump = { id: 10, value: {} }
  messages.next({
    value: { jsonrpc: '2.0', result: dump, meta: { patchSequence: 999 } },
    bytes: 80,
  })
  await flush()
  assert.deepEqual(received, [dump])
  assert.equal(f.cache.value.id, 0)
  messages.next({
    value: {
      rpcstream: '1.0',
      end: true,
      error: { code: 34, message: 'revoked' },
    },
    bytes: 80,
  })
  await flush()
  assert.equal(failure.code, 34)
  assert.equal(f.rejected(), 1)
  assert.equal(subscription.closed, true)
  assert.equal(messages.observers.length, 0)
  assert.equal(
    f.calls.some(call => call.transport === 'http'),
    false,
  )
})

test('normal log backlog survives a held initial fence without blocking a sibling', async context => {
  const f = fixture(context)
  const messages = new Subject()
  const { rpcResponseStream } = await import(
    new URL('services/rpc-stream.ts', shared)
  )
  f.sockets.stream = (body, identity, options) => {
    assert.equal(JSON.parse(body).method, 'server.logs.follow')
    return rpcResponseStream(messages, options)
  }
  const push = value =>
    messages.next({
      value,
      bytes: new TextEncoder().encode(JSON.stringify(value)).byteLength,
    })
  let adopted = false
  const pending = f.api.streams.server.logs.follow({}).then(response => {
    adopted = true
    return response
  })
  await flush()
  push({
    jsonrpc: '2.0',
    result: { guid: null, startCursor: 'start' },
    meta: { patchSequence: 7 },
  })
  await flush()
  assert.equal(adopted, false)
  const entries = Array.from({ length: 64 }, (_, n) => ({
    message: `backlog-${n}`,
    cursor: `cursor-${n}`,
    timestamp: '2026-11-12T00:00:00Z',
  }))
  for (const result of entries) push({ rpcstream: '1.0', result, end: false })
  assert.equal(await f.api.rpc.echo({ message: 'sibling' }), 'encrypted')
  assert.equal(adopted, false)
  assert.equal(f.cache.value.id, 0)
  assert.equal(messages.observers.length, 1)
  f.cache.next({ id: 7, value: {} })
  const response = await pending
  assert.equal(adopted, true)
  for (const entry of entries)
    assert.deepEqual(await response.items.next(), { done: false, value: entry })
  push({ rpcstream: '1.0', result: null, end: true })
  assert.deepEqual(await response.items.next(), { done: true, value: null })
  assert.equal(messages.observers.length, 0)
  assert.equal(f.rejected(), 0)
})

test('encrypted mutations still wait for the local committed sequence', async context => {
  const f = fixture(context)
  f.sockets.message = async () => ({
    result: 'saved',
    meta: { patchSequence: 4 },
  })
  let completed = false
  const pending = f.api.rpc.echo({ message: 'test' }).then(value => {
    completed = true
    return value
  })
  await flush()
  f.cache.next({ id: 3, value: {} })
  await flush()
  assert.equal(completed, false)
  f.cache.next({ id: 4, value: {} })
  assert.equal(await pending, 'saved')
})

test('startup errors preserve RPC details without discarding unrelated enrollment', async context => {
  const f = fixture(context)
  f.sockets.message = async () => ({
    error: {
      code: 34,
      message: 'startup failure',
      data: { details: 'details' },
    },
  })
  await assert.rejects(
    f.startup.rpc.diagnostic.error({}),
    error =>
      error.code === 34 &&
      error.message === 'RPC ERROR: startup failure\n\ndetails',
  )
  assert.equal(f.rejected(), 0)
})

test('HTTP resources preserve prepared-transfer cancellation, headers and signatures', async context => {
  const f = fixture(context)
  const signal = new AbortController().signal
  const file = new Blob(['file'])
  const progress = () => {}
  await f.api.uploadFile('upload-guid', file, {
    filename: 'a.txt',
    signal,
    progress,
  })
  const upload = f.calls.at(-1)
  assert.equal(upload.options.url, '/rest/rpc/upload-guid')
  assert.equal(upload.options.signal, signal)
  assert.equal(upload.options.uploadProgress, progress)
  assert.equal(upload.options.timeout, 0)
  assert.equal(upload.options.headers['ngsw-bypass'], 'true')
  assert.equal(upload.options.headers['X-File-Name'], 'a.txt')
  assert.equal(
    f.api.downloadFileUrl('download-guid'),
    '/rest/rpc/download-guid?ngsw-bypass=true',
  )
  await f.api.getStatic(['/s9pk/file'], {})
  assert.equal(f.calls.at(-2).transport, 'header')
  assert.equal(
    f.calls.at(-1).options.headers.get('X-Start-Auth-Sig'),
    'resource proof',
  )
})
