import assert from 'node:assert/strict'
import test from 'node:test'
import '@angular/compiler'
import { DOCUMENT, Injector } from '@angular/core'
import { auth } from '@start9labs/start-core'
import './typescript-loader.mjs'
const { RpcSocketService } =
  await import('../src/services/rpc-socket.service.ts')
const { AuthKeyService, AUTH_SIGNING_HOST } =
  await import('../src/services/auth-key.service.ts')
const { RELATIVE_URL } = await import('../src/tokens/relative-url.ts')

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
    assert.match(
      native.url,
      /\/ws\/rpc\/continuation\?noise=1&context=server.test$/,
    )
    socket.close()
    await closed.promise
    assert.ok(native.closed)
  },
)
