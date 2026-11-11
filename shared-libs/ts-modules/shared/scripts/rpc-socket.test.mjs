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

test('rpcstream initialization/events/terminal are scoped to one request and preserve a pending sibling', async context => {
  const { service, lease } = await streamFixture(context)
  const events = []
  let ended = false
  service.stream(body, lease).subscribe({
    next: value => events.push(value.result),
    complete: () => {
      ended = true
    },
  })
  const sibling = service.message(body, {}, lease)
  await tick()
  const socket = LogicalSocket.instances[0]
  const [stream, call] = socket.writes
  assert.equal(stream.rpcstream, version)
  socket.receive({ jsonrpc: '2.0', id: stream.id, result: 'initial' })
  socket.receive({
    rpcstream: version,
    id: stream.id,
    result: 'event',
    end: false,
  })
  socket.receive({ rpcstream: version, id: stream.id, result: null, end: true })
  assert.deepEqual(events, ['initial', 'event'])
  assert.equal(ended, true)
  assert.equal(socket.closed, 0)
  assert.ok(socket.writes.some(value => value.close === stream.id))
  socket.receive({ jsonrpc: '2.0', id: call.id, result: 'sibling' })
  assert.equal((await sibling).result, 'sibling')
})

test('rpcstream terminal application errors remain application envelopes, not transport failures', async context => {
  const { service, lease } = await streamFixture(context)
  const received = []
  let ended = false
  service.stream(body, lease).subscribe({
    next: value => received.push(value),
    complete: () => {
      ended = true
    },
  })
  await tick()
  const socket = LogicalSocket.instances[0]
  const { id } = socket.writes[0]
  socket.receive({ jsonrpc: '2.0', id, result: null })
  socket.receive({
    rpcstream: version,
    id,
    error: { code: 34, message: 'revoked' },
    end: true,
  })
  assert.equal(received.at(-1).error.code, 34)
  assert.equal(ended, true)
})

test('disposing pending initialization sends close without canceling or reconnecting its sibling', async context => {
  const { service, lease } = await streamFixture(context)
  const pending = service.stream(body, lease).subscribe()
  const events = []
  const sibling = service
    .stream(body, lease)
    .subscribe(value => events.push(value.result))
  await tick()
  const socket = LogicalSocket.instances[0]
  const [a, b] = socket.writes
  pending.unsubscribe()
  assert.ok(socket.writes.some(value => value.close === a.id))
  assert.equal(socket.closed, 0)
  socket.receive({ jsonrpc: '2.0', id: b.id, result: 'kept' })
  assert.deepEqual(events, ['kept'])
  sibling.unsubscribe()
  await tick()
  assert.equal(LogicalSocket.instances.length, 1)
})

test('transport loss ends every stream and pending call; only a new subscriber reconnects', async context => {
  const { service, lease } = await streamFixture(context)
  const errors = []
  service.stream(body, lease).subscribe({ error: error => errors.push(error) })
  service.stream(body, lease).subscribe({ error: error => errors.push(error) })
  const call = service.message(body, {}, lease).catch(error => error)
  await tick()
  LogicalSocket.instances[0].fail()
  network(await call)
  assert.equal(errors.length, 2)
  errors.forEach(network)
  await tick()
  assert.equal(LogicalSocket.instances.length, 1)
  const fresh = service.stream(body, lease).subscribe()
  await tick()
  assert.equal(LogicalSocket.instances.length, 2)
  fresh.unsubscribe()
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
