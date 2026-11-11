import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { DOCUMENT, Injector } from '@angular/core'
import { auth } from '@start9labs/start-core'
import { SESSION_PROFILE } from '@start9labs/start-core/osBindings/session-profile'
import { webSocket } from 'rxjs/webSocket'
import './typescript-loader.mjs'
const { RpcSocketService } =
  await import('../src/services/rpc-socket.service.ts')
const { AuthKeyService, AUTH_SIGNING_HOST } =
  await import('../src/services/auth-key.service.ts')
const { RELATIVE_URL } = await import('../src/tokens/relative-url.ts')
const { ServerIdentityService } =
  await import('../src/services/server-identity.service.ts')

for (const name of ['CloseEvent', 'ErrorEvent'])
  globalThis[name] ??= class extends Event {
    constructor(type, init = {}) {
      super(type)
      Object.assign(this, init)
    }
  }

class Socket extends EventTarget {
  static OPEN = 1
  static created
  bufferedAmount = 0
  readyState = 0
  closed = 0
  constructor(url) {
    super()
    this.url = url
    Socket.created.resolve(this)
  }
  close() {
    this.closed++
    this.readyState = 3
    this.dispatchEvent(new CloseEvent('close'))
  }
  send() {}
}

async function fixture(context) {
  const original = globalThis.WebSocket
  globalThis.WebSocket = Socket
  context.after(() => {
    globalThis.WebSocket = original
  })
  Socket.created = Promise.withResolvers()
  const lease = {
    key: await auth.generateAuthKey(),
    signal: new AbortController().signal,
  }
  const injector = Injector.create({
    providers: [
      RpcSocketService,
      {
        provide: DOCUMENT,
        useValue: {
          location: { origin: 'https://server.test' },
          defaultView: { crypto },
        },
      },
      { provide: RELATIVE_URL, useValue: '/rpc/v1' },
      { provide: AUTH_SIGNING_HOST, useValue: 'server.test' },
      { provide: AuthKeyService, useValue: { lease: async () => lease } },
      { provide: ServerIdentityService, useValue: { observe: async () => {} } },
    ],
  })
  const service = injector.get(RpcSocketService)
  context.after(() => {
    service.disconnect()
    injector.destroy()
  })
  return { service, lease }
}
const body = '{"method":"echo","params":{}}'
const network = error => assert.equal(error.code, 0)

class LogicalSocket {
  static instances = []
  readyState = 0
  writes = []
  closed = 0
  constructor() {
    LogicalSocket.instances.push(this)
    queueMicrotask(() => {
      this.readyState = 1
      this.onopen?.(new Event('open'))
    })
  }
  send(body) {
    this.writes.push(JSON.parse(body))
  }
  receive(body) {
    this.onmessage?.({ data: JSON.stringify(body) })
  }
  close() {
    this.closed++
    this.readyState = 3
    this.onclose?.({ wasClean: true })
  }
  fail() {
    this.onerror?.(new ErrorEvent('error', { error: Error('transport') }))
  }
}

async function streamFixture(context) {
  const result = await fixture(context)
  LogicalSocket.instances = []
  const subject = webSocket({
    url: 'ws://fixture',
    WebSocketCtor: LogicalSocket,
    serializer: value =>
      typeof value === 'string' ? value : JSON.stringify(value),
    deserializer: event => ({
      value: JSON.parse(event.data),
      bytes: new TextEncoder().encode(event.data).byteLength,
    }),
  })
  result.service.session = {
    signal: result.lease.signal,
    abort: new AbortController(),
    socket: subject,
  }
  context.after(() => subject.unsubscribe())
  return result
}

const version = SESSION_PROFILE.transport.rpcStreamVersion
const tick = () => new Promise(setImmediate)

test('typed initial/item/final values and metadata survive without canceling a sibling', async context => {
  const { service, lease } = await streamFixture(context)
  const pending = service.stream(body, lease)
  const sibling = service.message(body, {}, lease)
  await tick()
  const socket = LogicalSocket.instances[0]
  const [stream, call] = socket.writes
  assert.equal(stream.rpcstream, version)
  socket.receive({
    jsonrpc: '2.0',
    id: stream.id,
    result: 'initial',
    meta: { patchSequence: 11 },
  })
  socket.receive({
    rpcstream: version,
    id: stream.id,
    result: 'event',
    end: false,
  })
  socket.receive({ rpcstream: version, id: stream.id, result: 57, end: true })
  const response = await pending
  assert.equal(response.initial, 'initial')
  assert.deepEqual(response.meta, { patchSequence: 11 })
  assert.deepEqual(await response.items.next(), { value: 'event', done: false })
  assert.deepEqual(await response.items.next(), { value: 57, done: true })
  assert.equal(socket.closed, 0)
  assert.ok(socket.writes.some(value => value.close === stream.id))
  socket.receive({ jsonrpc: '2.0', id: call.id, result: 'sibling' })
  assert.equal((await sibling).result, 'sibling')
})

test('terminal application errors follow accepted items rather than becoming transport loss', async context => {
  const { service, lease } = await streamFixture(context)
  const pending = service.stream(body, lease)
  await tick()
  const socket = LogicalSocket.instances[0]
  const { id } = socket.writes[0]
  socket.receive({ jsonrpc: '2.0', id, result: null })
  socket.receive({ rpcstream: version, id, result: 'accepted', end: false })
  socket.receive({
    rpcstream: version,
    id,
    error: { code: 34, message: 'revoked' },
    end: true,
  })
  const { items } = await pending
  assert.deepEqual(await items.next(), { value: 'accepted', done: false })
  await assert.rejects(items.next(), error => error.code === 34)
})

test('aborting pending initialization and returning before first next dispose only their own IDs', async context => {
  const { service, lease } = await streamFixture(context)
  const abort = new AbortController()
  const pending = service.stream(body, lease, { signal: abort.signal })
  const rejected = assert.rejects(pending, { name: 'AbortError' })
  const sibling = service.stream(body, lease)
  await tick()
  const socket = LogicalSocket.instances[0]
  const [a, b] = socket.writes
  abort.abort()
  await rejected
  assert.ok(socket.writes.some(value => value.close === a.id))
  assert.equal(socket.closed, 0)
  socket.receive({ jsonrpc: '2.0', id: b.id, result: 'kept' })
  const response = await sibling
  assert.equal(response.initial, 'kept')
  await response.items.return(null)
  assert.ok(socket.writes.some(value => value.close === b.id))
  await tick()
  assert.equal(LogicalSocket.instances.length, 1)
})

test('transport loss cancels two established streams, pending initialization and a call without replay', async context => {
  const { service, lease } = await streamFixture(context)
  const a = service.stream(body, lease)
  const b = service.stream(body, lease)
  const pending = service.stream(body, lease).catch(error => error)
  const call = service.message(body, {}, lease).catch(error => error)
  await tick()
  const socket = LogicalSocket.instances[0]
  socket.receive({ jsonrpc: '2.0', id: socket.writes[0].id, result: null })
  socket.receive({ jsonrpc: '2.0', id: socket.writes[1].id, result: null })
  const first = (await a).items.next().catch(error => error)
  const second = (await b).items.next().catch(error => error)
  socket.fail()
  for (const result of await Promise.all([first, second, pending, call]))
    network(result)
  await tick()
  assert.equal(LogicalSocket.instances.length, 1)
  const abort = new AbortController()
  const fresh = service
    .stream(body, lease, { signal: abort.signal })
    .catch(error => error)
  await tick()
  assert.equal(LogicalSocket.instances.length, 2)
  abort.abort()
  assert.equal((await fresh).name, 'AbortError')
})

test('a slow stream fails at the bounded queue while its sibling completes', async context => {
  const { service, lease } = await streamFixture(context)
  const pending = service.stream(body, lease)
  const sibling = service.message(body, {}, lease)
  await tick()
  const socket = LogicalSocket.instances[0]
  const [stream, call] = socket.writes
  socket.receive({ jsonrpc: '2.0', id: stream.id, result: null })
  const { items } = await pending
  for (let n = 0; n <= SESSION_PROFILE.transport.maxQueuedStreamEvents; n++)
    socket.receive({ rpcstream: version, id: stream.id, result: n, end: false })
  await assert.rejects(
    items.next(),
    error => error.code === SESSION_PROFILE.transport.rpcStreamCancelledCode,
  )
  assert.equal(socket.closed, 0)
  socket.receive({ jsonrpc: '2.0', id: call.id, result: 'unblocked' })
  assert.equal((await sibling).result, 'unblocked')
})

test('a stream initialization deadline closes only its own ID', async context => {
  const { service, lease } = await streamFixture(context)
  const sibling = service.message(body, {}, lease)
  const expired = service
    .stream(body, lease, { timeout: 10 })
    .catch(error => error)
  await tick()
  const socket = LogicalSocket.instances[0]
  const [call, stream] = socket.writes
  network(await expired)
  assert.ok(socket.writes.some(value => value.close === stream.id))
  assert.equal(socket.closed, 0)
  socket.receive({ jsonrpc: '2.0', id: call.id, result: 'survived' })
  assert.equal((await sibling).result, 'survived')
})

test(
  'disconnect aborts every pending RPC and the next call opens a fresh socket',
  { timeout: 3000 },
  async context => {
    const { service, lease } = await fixture(context)
    const first = service.message(body, {}, lease).catch(error => error)
    const second = service.message(body, {}, lease).catch(error => error)
    const socket = await Socket.created.promise
    service.disconnect()
    network(await first)
    network(await second)
    assert.ok(socket.closed)
    Socket.created = Promise.withResolvers()
    const next = service.message(body, {}, lease).catch(error => error)
    const fresh = await Socket.created.promise
    assert.notEqual(fresh, socket)
    service.disconnect()
    network(await next)
  },
)

test(
  'a probe deadline cancels its request; outage teardown also aborts a held sibling',
  { timeout: 3000 },
  async context => {
    const { service, lease } = await fixture(context)
    const probe = service
      .message(body, { timeout: 20 }, lease)
      .catch(error => error)
    const sibling = service.message(body, {}, lease).catch(error => error)
    const socket = await Socket.created.promise
    network(await probe)
    assert.equal(socket.closed, 0)
    service.disconnect()
    network(await sibling)
    assert.ok(socket.closed)
  },
)

test(
  'DOM continuation sockets retain caller event handlers and close ownership',
  { timeout: 3000 },
  async context => {
    const { service } = await fixture(context)
    const socket = service.openSocket('continuation')
    const closed = Promise.withResolvers()
    socket.onclose = event => closed.resolve(event)
    const native = await Socket.created.promise
    assert.match(native.url, /\/rpc\/v2\/continuation$/)
    socket.close()
    await closed.promise
    assert.ok(native.closed)
  },
)
