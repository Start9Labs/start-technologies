import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'
import { auth } from '@start9labs/start-core'
import { SESSION_PROFILE } from '@start9labs/start-core/osBindings/session-profile'
import './typescript-loader.mjs'

// Framing/lifecycle tests replace crypto; real signed-DH interoperability has separate coverage.
registerHooks({
  load(url, context, next) {
    if (!url.endsWith('/session-identity.ts')) return next(url, context)
    return {
      format: 'module',
      shortCircuit: true,
      source: `
      export async function prepareSession() {
        return { message: new Uint8Array([1]), async finish() {
          return { peer: ${JSON.stringify(auth.ed25519PublicKeyDer64(new Uint8Array(32).fill(7)))}, cipher: {
            async write(bytes) { return bytes.slice() }, async read(bytes) { await globalThis.sessionRecordGate?.(); return bytes.slice() }
          } }
        } }
      }
    `,
    }
  },
})
const { SessionWebSocketStream } =
  await import('../src/services/session-stream.ts')
const { sessionWebSocketConstructor } =
  await import('../src/services/session-websocket.ts')
const profile = SESSION_PROFILE.transport
for (const name of ['CloseEvent', 'ErrorEvent'])
  globalThis[name] ??= class extends Event {
    constructor(type, init = {}) {
      super(type)
      Object.assign(this, { code: 0, reason: '', wasClean: false }, init)
    }
  }

class Socket extends EventTarget {
  static instances = []
  bufferedAmount = 0
  closed = 0
  writes = []
  constructor(url) {
    super()
    this.url = url
    Socket.instances.push(this)
  }
  send(data) {
    this.writes.push(data)
  }
  close() {
    if (this.closed++) return
    this.dispatchEvent(new Event('close'))
  }
  receive(data) {
    this.dispatchEvent(new MessageEvent('message', { data }))
  }
}
const identity = () => ({
  signer: {
    sign() {
      throw Error('Unexpected signing')
    },
  },
  signal: new AbortController().signal,
})
function record(bytes) {
  const frame = new Uint8Array(2 + bytes.length)
  new DataView(frame.buffer).setUint16(0, bytes.length)
  frame.set(bytes, 2)
  return frame.buffer
}
async function framedSocket(context) {
  class Open extends Socket {
    constructor(url) {
      super(url)
      queueMicrotask(() => this.dispatchEvent(new Event('open')))
    }
    send(data) {
      super.send(data)
      if (this.writes.length === 1)
        queueMicrotask(() => this.receive(record([1])))
    }
  }
  const lifetime = new AbortController()
  const Constructor = sessionWebSocketConstructor(
    async () => ({ ...identity(), signal: lifetime.signal }),
    'server.test',
    'rpc',
    Open,
  )
  const socket = new Constructor('ws://server.test/rpc/v2')
  const opened = await new Promise(resolve =>
    socket.addEventListener('open', resolve, { once: true }),
  )
  const raw = Socket.instances.at(-1)
  context.after(() => raw.close())
  return {
    socket,
    opened,
    raw,
    lifetime,
    server: {
      send: bytes => raw.receive(record(bytes)),
      close: () => raw.close(),
      abort: () => raw.dispatchEvent(new Event('error')),
    },
  }
}
function closed(socket) {
  return new Promise(resolve =>
    socket.addEventListener('close', resolve, { once: true }),
  )
}

test('cancelling during signer preparation never constructs a WebSocket', async () => {
  Socket.instances = []
  let resolve
  const Constructor = sessionWebSocketConstructor(
    () =>
      new Promise(done => {
        resolve = done
      }),
    'server.test',
    'rpc',
    Socket,
  )
  const socket = new Constructor('ws://server.test/rpc/v2')
  socket.close()
  resolve(identity())
  await Promise.resolve()
  await Promise.resolve()
  assert.equal(Socket.instances.length, 0)
  assert.equal(socket.readyState, socket.CLOSED)
})

test('plaintext downgrade never opens admission or exposes RPC data', async () => {
  class Downgrade extends Socket {
    constructor(url) {
      super(url)
      queueMicrotask(() => this.dispatchEvent(new Event('open')))
    }
    send(data) {
      super.send(data)
      queueMicrotask(() => this.receive('plaintext downgrade'))
    }
  }
  const Constructor = sessionWebSocketConstructor(
    async () => identity(),
    'server.test',
    'rpc',
    Downgrade,
  )
  const socket = new Constructor('ws://server.test/rpc/v2?existing=kept')
  let opened = 0
  socket.addEventListener('open', () => opened++)
  const result = await closed(socket)
  assert.equal(opened, 0)
  assert.equal(result.code, 1006)
  assert.equal(result.wasClean, false)
  const raw = Socket.instances.at(-1)
  assert.ok(raw.closed)
  assert.ok(raw.writes.every(value => typeof value !== 'string'))
  const url = new URL(raw.url)
  assert.equal(url.searchParams.has('secure'), false)
  assert.equal(url.searchParams.has('context'), false)
  assert.equal(url.searchParams.get('existing'), 'kept')
})

for (const [name, bytes] of [
  ['plaintext', 'unexpected text'],
  ['oversized raw chunk', new ArrayBuffer(profile.maxWireChunkBytes + 1)],
])
  test(`raw adapter rejects ${name}`, async () => {
    const socket = new Socket('ws://server.test')
    const raw = new SessionWebSocketStream(socket)
    const read = raw.read(1, new AbortController().signal)
    socket.receive(bytes)
    await assert.rejects(read)
    assert.ok(socket.closed)
  })

test('handshake bound survives a split record header without oversized allocation', async () => {
  const socket = new Socket('ws://server.test')
  const raw = new SessionWebSocketStream(socket)
  const read = raw.readRecord(
    profile.maxHandshakeRecordBytes,
    new AbortController().signal,
  )
  socket.receive(Uint8Array.of(255).buffer)
  socket.receive(Uint8Array.of(255).buffer)
  await assert.rejects(read, /Oversized/)
  raw.abort(Error('test complete'))
})

test('open carries immutable identity from completed admission', async context => {
  const { socket, opened } = await framedSocket(context)
  assert.equal(
    opened.peer,
    auth.ed25519PublicKeyDer64(new Uint8Array(32).fill(7)),
  )
  assert.equal(socket.readyState, socket.OPEN)
  socket.close()
})

test('logical text retains its leading UTF-8 BOM', async context => {
  const { socket, server } = await framedSocket(context)
  const message = new Promise(resolve =>
    socket.addEventListener('message', resolve, { once: true }),
  )
  server.send(Uint8Array.of(0, 0, 0, 7, 0, 239, 187, 191, 65, 66, 67))
  assert.equal((await message).data, '\uFEFFABC')
  socket.close()
})

test('full data write budget leaves an independent close budget', async context => {
  const { socket } = await framedSocket(context)
  socket.send(new Blob([new ArrayBuffer(profile.maxIncomingBytes)]))
  socket.close(1000, 'done')
  assert.equal(socket.readyState, socket.CLOSING)
})

for (const [name, local, fail] of [
  ['transport ends after local close', true, server => server.close()],
  ['active transport loss', false, server => server.close()],
  [
    'malformed data after local close',
    true,
    server => server.send(Uint8Array.of(0, 0, 0, 2, 0, 255)),
  ],
  ['failed close write', true, server => server.abort()],
])
  test(`${name} never fabricates a verified result or stale error`, async context => {
    const { socket, server } = await framedSocket(context)
    let errors = 0
    socket.addEventListener('error', () => errors++)
    const result = closed(socket)
    if (local) socket.close()
    fail(server)
    const event = await result
    assert.equal(errors, local ? 0 : 1)
    assert.equal(event.code, 1006)
    assert.equal(event.wasClean, false)
  })

for (const code of [1000, 1008, 1011]) {
  test(`authenticated close ${code} is transport loss, not an application outcome`, async context => {
    const { socket, server } = await framedSocket(context)
    let errors = 0
    socket.addEventListener('error', () => errors++)
    const result = closed(socket)
    server.send(
      Uint8Array.of(
        0,
        0,
        0,
        8,
        2,
        code >> 8,
        code & 255,
        239,
        187,
        191,
        79,
        75,
      ),
    )
    const event = await result
    server.abort()
    assert.equal(errors, 1)
    assert.equal(event.code, 1006)
    assert.equal(event.reason, '')
    assert.equal(event.wasClean, false)
  })
}

for (const [name, bytes] of [
  ['reserved close status', [0, 0, 0, 3, 2, 3, 237]],
  ['invalid UTF-8', [0, 0, 0, 2, 0, 255]],
  ['data after close', [0, 0, 0, 3, 2, 3, 232, 0, 0, 0, 2, 0, 65]],
  ['oversized logical message', [255, 255, 255, 255]],
])
  test(`logical framing rejects ${name}`, async context => {
    const { socket, server } = await framedSocket(context)
    const result = closed(socket)
    server.send(Uint8Array.from(bytes))
    assert.equal((await result).code, 1006)
  })

test('raw disposal releases queued bytes even after physical EOF', () => {
  const socket = new Socket('ws://server.test')
  const raw = new SessionWebSocketStream(socket)
  socket.receive(new ArrayBuffer(4000))
  assert.equal(raw.readBufferLength, 4000)
  socket.close()
  raw.abort(Error('dispose'))
  assert.equal(raw.readBufferLength, 0)
})

test('key cancellation during asynchronous decryption cannot dispatch a late message', async context => {
  const { socket, server, lifetime } = await framedSocket(context)
  const entered = Promise.withResolvers()
  const gate = Promise.withResolvers()
  globalThis.sessionRecordGate = () => {
    entered.resolve()
    return gate.promise
  }
  context.after(() => {
    delete globalThis.sessionRecordGate
  })
  let messages = 0
  socket.addEventListener('message', () => messages++)
  const result = closed(socket)
  server.send(Uint8Array.of(0, 0, 0, 2, 0, 65))
  await entered.promise
  lifetime.abort()
  gate.resolve()
  assert.equal((await result).code, 1006)
  await Promise.resolve()
  await Promise.resolve()
  assert.equal(messages, 0)
})

test('raw writes wait for bufferedAmount and abort without a retained drain task', async () => {
  const socket = new Socket('ws://server.test')
  const raw = new SessionWebSocketStream(socket)
  socket.bufferedAmount = profile.maxWireChunkBytes
  const abort = new AbortController()
  const write = raw.writeRecord(Uint8Array.of(1, 2, 3), abort.signal)
  assert.equal(socket.writes.length, 0)
  abort.abort()
  await assert.rejects(write)
  raw.abort(Error('test complete'))
})
