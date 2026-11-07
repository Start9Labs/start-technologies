import assert from 'node:assert/strict'
import test from 'node:test'
import { auth } from '@start9labs/start-core'
import { NOISE_PROFILE } from '@start9labs/start-core/osBindings/noise-profile'
import { Uint8ArrayList } from 'uint8arraylist'
import { streamPair } from '@libp2p/utils'
import './typescript-loader.mjs'

const { prepareNoiseIdentity } =
  await import('../src/services/noise-identity.ts')
const { NoiseWebSocketStream } = await import('../src/services/noise-stream.ts')
const { noiseWebSocketConstructor } =
  await import('../src/services/noise-websocket.ts')

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

test('identity preparation keeps the private key opaque and signs before connection', async context => {
  const key = await auth.generateAuthKey()
  const sign = context.mock.method(crypto.subtle, 'sign')
  const exported = context.mock.method(crypto.subtle, 'exportKey')
  const identity = await prepareNoiseIdentity(key)
  assert.equal(key.privateKey.extractable, false)
  assert.equal(sign.mock.callCount(), 1)
  assert.equal(sign.mock.calls[0].arguments[1], key.privateKey)
  assert.equal(exported.mock.callCount(), 0)
  assert.equal(typeof identity, 'function')
  assert.equal(sign.mock.callCount(), 1)
})

test('cancelling during identity preparation never constructs a WebSocket', async () => {
  Socket.instances = []
  let prepared
  const Constructor = noiseWebSocketConstructor(
    () =>
      new Promise(resolve => {
        prepared = resolve
      }),
    'server.test',
    Socket,
  )
  const socket = new Constructor('ws://server.test/ws/rpc')
  assert.equal(Socket.instances.length, 0)
  socket.close()
  prepared({
    identity: () => {
      throw new Error('unexpected handshake')
    },
    signal: new AbortController().signal,
  })
  await Promise.resolve()
  await Promise.resolve()
  assert.equal(Socket.instances.length, 0)
  assert.equal(socket.readyState, socket.CLOSED)
})

test('a plaintext downgrade closes negotiation without opening or exposing RPC data', async () => {
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
  const identity = await prepareNoiseIdentity(await auth.generateAuthKey())
  const Constructor = noiseWebSocketConstructor(
    async () => ({ identity, signal: new AbortController().signal }),
    'server.test',
    Downgrade,
  )
  const socket = new Constructor('ws://server.test/ws/rpc?existing=kept')
  let opened = 0
  socket.addEventListener('open', () => {
    opened++
  })
  const close = await new Promise(resolve =>
    socket.addEventListener('close', resolve, { once: true }),
  )
  assert.equal(opened, 0)
  assert.equal(close.code, NOISE_PROFILE.transport.abnormalCloseCode)
  assert.equal(close.wasClean, false)
  const raw = Socket.instances.at(-1)
  assert.ok(raw.closed)
  assert.ok(raw.writes.length)
  assert.ok(raw.writes.every(data => typeof data !== 'string'))
  const url = new URL(raw.url)
  assert.equal(url.searchParams.get('context'), 'server.test')
  assert.equal(url.searchParams.get('existing'), 'kept')
})

for (const [name, bytes] of [
  ['plaintext', 'unexpected text'],
  [
    'oversized raw chunk',
    new ArrayBuffer(NOISE_PROFILE.transport.maxWireChunkBytes + 1),
  ],
  ['oversized handshake record', Uint8Array.of(255, 255).buffer],
])
  test(`the raw adapter rejects ${name}`, () => {
    const socket = new Socket('ws://server.test')
    new NoiseWebSocketStream(socket)
    socket.receive(bytes)
    assert.ok(socket.closed)
  })

test('handshake length validation survives a split record header', () => {
  const socket = new Socket('ws://server.test')
  new NoiseWebSocketStream(socket)
  socket.receive(Uint8Array.of(255).buffer)
  assert.equal(socket.closed, 0)
  socket.receive(Uint8Array.of(255).buffer)
  assert.ok(socket.closed)
})

async function framedSocket(context) {
  const [client, server] = await streamPair({ delay: 0 })
  context.after(() => {
    client.abort(new Error('test complete'))
    server.abort(new Error('test complete'))
  })
  class Open extends Socket {
    constructor(url) {
      super(url)
      queueMicrotask(() => this.dispatchEvent(new Event('open')))
    }
  }
  const Constructor = noiseWebSocketConstructor(
    async () => ({
      signal: new AbortController().signal,
      identity: async () => ({
        connection: client,
        remotePeer: { publicKey: { type: 'Ed25519' } },
      }),
    }),
    'server.test',
    Open,
  )
  const socket = new Constructor('ws://server.test')
  await new Promise(resolve =>
    socket.addEventListener('open', resolve, { once: true }),
  )
  return { socket, server }
}

test('logical text retains its leading UTF-8 BOM', async context => {
  const { socket, server } = await framedSocket(context)
  const message = new Promise(resolve =>
    socket.addEventListener('message', resolve, { once: true }),
  )
  server.send(Uint8Array.of(0, 0, 0, 7, 0, 239, 187, 191, 65, 66, 67))
  assert.equal((await message).data, '\uFEFFABC')
  socket.close()
})

test('a full data write budget still permits an independently bounded close', async context => {
  const { socket } = await framedSocket(context)
  socket.send(
    new Blob([new ArrayBuffer(NOISE_PROFILE.transport.maxIncomingBytes)]),
  )
  socket.close(1000, 'done')
  assert.equal(socket.readyState, socket.CLOSING)
})

test('authenticated close reasons retain their leading UTF-8 BOM', async context => {
  const { socket, server } = await framedSocket(context)
  const close = new Promise(resolve =>
    socket.addEventListener('close', resolve, { once: true }),
  )
  server.send(Uint8Array.of(0, 0, 0, 8, 2, 3, 232, 239, 187, 191, 79, 75))
  const result = await close
  assert.equal(result.code, 1000)
  assert.equal(result.reason, '\uFEFFOK')
  assert.equal(result.wasClean, true)
})

for (const [name, frame] of [
  ['reserved close status', [0, 0, 0, 3, 2, 3, 237]],
  ['invalid text UTF-8', [0, 0, 0, 2, 0, 255]],
  ['data after close', [0, 0, 0, 3, 2, 3, 232, 0, 0, 0, 2, 0, 65]],
])
  test(`logical framing rejects ${name}`, async context => {
    const { socket, server } = await framedSocket(context)
    const close = new Promise(resolve =>
      socket.addEventListener('close', resolve, { once: true }),
    )
    server.send(Uint8Array.from(frame))
    const result = await close
    assert.equal(result.code, 1006)
    assert.equal(result.wasClean, false)
  })

test('raw writes honor browser bufferedAmount and stop the drain task on close', () => {
  const socket = new Socket('ws://server.test')
  const raw = new NoiseWebSocketStream(socket)
  socket.bufferedAmount = NOISE_PROFILE.transport.maxWireChunkBytes
  const bytes = Uint8Array.of(1, 2, 3)
  assert.deepEqual(raw.sendData(new Uint8ArrayList(bytes)), {
    sentBytes: 3,
    canSendMore: false,
  })
  assert.deepEqual(socket.writes[0], bytes)
  socket.close()
  assert.equal(socket.closed, 1)
})
