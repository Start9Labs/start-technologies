import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'
import { auth } from '@start9labs/start-core'
import { SESSION_PROFILE } from '@start9labs/start-core/osBindings/session-profile'
import { SIGNATURE_PROFILE } from '@start9labs/start-core/osBindings/signature-profile'
import './typescript-loader.mjs'

// Framing/lifecycle tests replace crypto; real signed-DH interoperability has separate coverage.
registerHooks({
  load(url, context, next) {
    if (!url.endsWith('/session-identity.ts')) return next(url, context)
    return {
      format: 'module',
      shortCircuit: true,
      source: `
      export async function prepareSession(...args) {
        globalThis.preparedSessions?.push(args)
        return { message: new Uint8Array([1]), async finish(response) {
          return { temporary: response[0] === 2, peer: ${JSON.stringify(auth.ed25519PublicKeyDer64(new Uint8Array(32).fill(7)))}, cipher: {
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
async function framedSocket(
  context,
  temporary = false,
  allowTemporaryIdentity = true,
) {
  class Open extends Socket {
    constructor(url) {
      super(url)
      queueMicrotask(() => this.dispatchEvent(new Event('open')))
    }
    send(data) {
      super.send(data)
      if (this.writes.length === 1)
        queueMicrotask(() => this.receive(record([temporary ? 2 : 1])))
    }
  }
  const lifetime = new AbortController()
  const Constructor = sessionWebSocketConstructor(
    async () => ({ ...identity(), signal: lifetime.signal }),
    'server.test',
    'rpc',
    Open,
    allowTemporaryIdentity,
  )
  const socket = new Constructor('ws://server.test/rpc/v2')
  context.after(() => socket.close())
  const opened = await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, { once: true })
    socket.addEventListener('error', event => reject(event.error), {
      once: true,
    })
  })
  const raw = Socket.instances.at(-1)
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

function admissionServer(answers) {
  return class extends Socket {
    constructor(url) {
      super(url)
      queueMicrotask(() => this.dispatchEvent(new Event('open')))
    }
    send(data) {
      super.send(data)
      if (this.writes.length !== 1) return
      const answer = answers.shift()
      queueMicrotask(() =>
        answer === undefined ? this.close() : this.receive(record(answer)),
      )
    }
  }
}
async function withServerClock(skewMs, run) {
  const fetch = globalThis.fetch
  const requests = []
  globalThis.preparedSessions = []
  globalThis.fetch = async (url, init) => {
    requests.push({ url: String(url), method: init.method })
    return new Response(null, {
      headers: { date: new Date(Date.now() + skewMs).toUTCString() },
    })
  }
  try {
    await run(requests, globalThis.preparedSessions)
  } finally {
    globalThis.fetch = fetch
    delete globalThis.preparedSessions
  }
}
function connect(host, Server) {
  const Constructor = sessionWebSocketConstructor(
    async () => identity(),
    'server.test',
    'rpc',
    Server,
  )
  return connectionResult(new Constructor(`wss://${host}/rpc/v2`))
}
function connectionResult(socket) {
  return new Promise(resolve => {
    socket.addEventListener('open', () => resolve({ socket, opened: true }), {
      once: true,
    })
    socket.addEventListener(
      'close',
      event => resolve({ socket, opened: false, event }),
      { once: true },
    )
  })
}

test('an unanswered admission is re-signed once with the server clock and remembered', async () => {
  await withServerClock(120_000, async (requests, prepared) => {
    Socket.instances = []
    const first = await connect(
      'skewed.test',
      admissionServer([undefined, [1]]),
    )
    assert.equal(first.opened, true)
    assert.equal(Socket.instances.length, 2)
    assert.deepEqual(requests, [
      { url: 'https://skewed.test/', method: 'HEAD' },
    ])
    const [initial, retried] = prepared.map(args => args[3] - Date.now())
    assert.ok(Math.abs(initial) < 1_000)
    assert.ok(Math.abs(retried - 120_000) < 2_000)
    first.socket.close()

    const second = await connect('skewed.test', admissionServer([[1]]))
    assert.equal(second.opened, true)
    assert.equal(requests.length, 1)
    assert.ok(Math.abs(prepared.at(-1)[3] - Date.now() - 120_000) < 2_000)
    second.socket.close()
  })
})

test('an unanswered admission within the signing tolerance is not retried', async () => {
  await withServerClock(5_000, async (requests, prepared) => {
    Socket.instances = []
    const result = await connect(
      'synced.test',
      admissionServer([undefined, [1]]),
    )
    assert.equal(result.opened, false)
    assert.equal(result.event.code, profile.abnormalCloseCode)
    assert.equal(Socket.instances.length, 1)
    assert.equal(requests.length, 1)
    assert.equal(prepared.length, 1)
  })
})

test('a socket that never opens never consults the server clock', async () => {
  class Unreachable extends Socket {
    constructor(url) {
      super(url)
      queueMicrotask(() => this.close())
    }
  }
  await withServerClock(120_000, async (requests, prepared) => {
    Socket.instances = []
    const result = await connect('unreachable.test', Unreachable)
    assert.equal(result.opened, false)
    assert.equal(Socket.instances.length, 1)
    assert.equal(requests.length, 0)
    assert.equal(prepared.length, 1)
  })
})

test('an answered admission failure never consults the server clock', async () => {
  await withServerClock(120_000, async (requests, prepared) => {
    Socket.instances = []
    const result = await connect('answered.test', admissionServer([[2], [1]]))
    assert.equal(result.opened, false)
    assert.equal(Socket.instances.length, 1)
    assert.equal(requests.length, 0)
    assert.equal(prepared.length, 1)
  })
})

test('a re-signed admission the server still drops is not remembered', async () => {
  await withServerClock(120_000, async (requests, prepared) => {
    Socket.instances = []
    const first = await connect(
      'disputed.test',
      admissionServer([undefined, undefined]),
    )
    assert.equal(first.opened, false)
    assert.equal(Socket.instances.length, 2)
    assert.equal(requests.length, 1)

    const second = await connect('disputed.test', admissionServer([[1]]))
    assert.equal(second.opened, true)
    assert.ok(Math.abs(prepared.at(-1)[3] - Date.now()) < 1_000)
    second.socket.close()
  })
})

test('a remembered offset follows a corrected server clock', async () => {
  await withServerClock(120_000, async () => {
    const skewed = await connect(
      'corrected.test',
      admissionServer([undefined, [1]]),
    )
    assert.equal(skewed.opened, true)
    skewed.socket.close()
  })
  await withServerClock(0, async (requests, prepared) => {
    Socket.instances = []
    const corrected = await connect(
      'corrected.test',
      admissionServer([undefined, [1]]),
    )
    assert.equal(corrected.opened, true)
    assert.equal(Socket.instances.length, 2)
    assert.equal(requests.length, 1)
    const [stale, fresh] = prepared.map(args => args[3] - Date.now())
    assert.ok(Math.abs(stale - 120_000) < 2_000)
    assert.ok(Math.abs(fresh) < 1_000)
    corrected.socket.close()

    const next = await connect('corrected.test', admissionServer([[1]]))
    assert.equal(next.opened, true)
    assert.equal(requests.length, 1)
    assert.ok(Math.abs(prepared.at(-1)[3] - Date.now()) < 1_000)
    next.socket.close()
  })
})

for (const skew of [-120_000, 120_000])
  test(`correcting a ${skew} ms device clock refreshes before single-use admission`, async () => {
    const now = Date.now
    let deviceSkew = skew
    Date.now = () => now() + deviceSkew
    const host = `corrected-device-${skew}.test`
    try {
      await withServerClock(-skew, async () => {
        const initial = await connect(host, admissionServer([undefined, [1]]))
        assert.equal(initial.opened, true)
        initial.socket.close()
      })
      deviceSkew = 0
      await withServerClock(0, async (requests, prepared) => {
        Socket.instances = []
        let available = true
        class SingleUse extends Socket {
          constructor(url) {
            super(url)
            const found = available
            available = false
            queueMicrotask(() =>
              found ? this.dispatchEvent(new Event('open')) : this.close(),
            )
          }
          send(data) {
            super.send(data)
            if (this.writes.length !== 1) return
            const timely =
              Math.abs(prepared.at(-1)[3] - Date.now()) <=
              SIGNATURE_PROFILE.timestampToleranceSecs * 1000
            queueMicrotask(() =>
              timely ? this.receive(record([1])) : this.close(),
            )
          }
        }
        const guid = 'A'.repeat(32)
        const Constructor = sessionWebSocketConstructor(
          async () => identity(),
          'server.test',
          { continuation: guid },
          SingleUse,
        )
        const result = await connectionResult(
          new Constructor(`wss://${host}/rpc/v2/${guid}`),
        )
        assert.equal(result.opened, true)
        assert.equal(Socket.instances.length, 1)
        assert.equal(prepared.length, 1)
        assert.deepEqual(requests, [
          { url: `https://${host}/`, method: 'HEAD' },
        ])
        result.socket.close()

        const next = await connect(host, admissionServer([[1]]))
        assert.equal(next.opened, true)
        assert.equal(requests.length, 1)
        assert.ok(Math.abs(prepared.at(-1)[3] - Date.now()) < 1_000)
        next.socket.close()
      })
    } finally {
      Date.now = now
    }
  })

test('closing while the server clock is read opens no second connection', async () => {
  const fetch = globalThis.fetch
  globalThis.preparedSessions = []
  let reading
  const started = new Promise(resolve => (reading = resolve))
  globalThis.fetch = (_url, init) =>
    new Promise((_, reject) => {
      reading()
      init.signal.addEventListener('abort', () => reject(init.signal.reason), {
        once: true,
      })
    })
  try {
    Socket.instances = []
    const Constructor = sessionWebSocketConstructor(
      async () => identity(),
      'server.test',
      'rpc',
      admissionServer([undefined, [1]]),
    )
    const socket = new Constructor('wss://closing.test/rpc/v2')
    const events = []
    for (const type of ['open', 'error', 'close'])
      socket.addEventListener(type, () => events.push(type))
    await started
    socket.close()
    await new Promise(resolve => setTimeout(resolve, 10))
    assert.deepEqual(events, ['close'])
    assert.equal(socket.readyState, socket.CLOSED)
    assert.equal(Socket.instances.length, 1)
    assert.equal(globalThis.preparedSessions.length, 1)
  } finally {
    globalThis.fetch = fetch
    delete globalThis.preparedSessions
  }
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

for (const temporary of [false, true])
  test(`open carries verified identity and temporary=${temporary} from admission`, async context => {
    const { socket, opened } = await framedSocket(context, temporary)
    assert.equal(
      opened.peer,
      auth.ed25519PublicKeyDer64(new Uint8Array(32).fill(7)),
    )
    assert.equal(opened.temporary, temporary)
    assert.equal(socket.readyState, socket.OPEN)
    socket.close()
  })

test('normal admission rejects a temporary identity before open or application writes', async context => {
  await assert.rejects(
    framedSocket(context, true, false),
    /Temporary server identity is not allowed/,
  )
  const raw = Socket.instances.at(-1)
  assert.equal(raw.writes.length, 1)
  assert.ok(raw.closed)
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
