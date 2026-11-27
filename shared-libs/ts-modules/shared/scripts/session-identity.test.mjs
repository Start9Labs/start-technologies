import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { auth } from '@start9labs/start-core'
import { x25519 } from '@noble/curves/ed25519'
import { decodeFirst, encode } from '@start9labs/start-core/auth/cbor'
import './typescript-loader.mjs'

const { prepareSession, retryTime } =
  await import('../src/services/session-identity.ts')
const fixture = JSON.parse(
  readFileSync(
    new URL(
      '../../../crates/start-core/testdata/signed-session/webcrypto.json',
      import.meta.url,
    ),
  ),
)
const bytes = value => new Uint8Array(Buffer.from(value, 'base64'))
const client = bytes(fixture.client)
const server = bytes(fixture.server)
const payload = client.subarray(0, (await decodeFirst(client)).length)
const proof = client.subarray(payload.length)
const encoder = new TextEncoder()
const concatenate = (...parts) => new Uint8Array(Buffer.concat(parts))

async function frozenClient(context) {
  const secret = Uint8Array.from(Buffer.from(fixture.clientPrivate, 'hex'))
  context.mock.method(x25519.utils, 'randomPrivateKey', () => secret.slice())
  return prepareSession(
    {
      sign: async (name, body) => {
        assert.equal(name, fixture.context)
        assert.deepEqual(body, payload)
        return proof
      },
    },
    fixture.context,
    'rpc',
  )
}

async function signedResponse(body) {
  const key = await auth.generateAuthKey()
  return concatenate(
    body,
    await auth.requestSigner(key).sign(fixture.context, body),
  )
}

test('opaque fresh initiation signs once per connection and binds exact GUID intent', async context => {
  const key = await auth.generateAuthKey()
  const sign = context.mock.method(crypto.subtle, 'sign')
  const exported = context.mock.method(crypto.subtle, 'exportKey')
  const signer = auth.requestSigner(key)
  const one = await prepareSession(signer, fixture.context, {
    continuation: 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA',
  })
  const two = await prepareSession(signer, fixture.context, 'rpc')
  assert.equal(sign.mock.callCount(), 2)
  assert.equal(key.privateKey.extractable, false)
  assert.ok(
    exported.mock.calls.every(call => call.arguments[1].type === 'public'),
  )
  const decode = async prepared => {
    const body = await decodeFirst(prepared.message)
    const proof = prepared.message.subarray(body.length)
    assert.equal(
      await auth.verifyRequestProof(
        proof,
        fixture.context,
        prepared.message.subarray(0, body.length),
      ),
      auth.normalizeDer64(key.pubkeyPem),
    )
    return {
      hello: body.value.hello,
      nonce: (await decodeFirst(proof)).value.commitment.nonce,
    }
  }
  const a = await decode(one),
    b = await decode(two)
  assert.notDeepEqual(a.hello.ephemeral, b.hello.ephemeral)
  assert.notEqual(BigInt(a.nonce), BigInt(b.nonce))
  assert.deepEqual(a.hello.target, {
    continuation: 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA',
  })
  assert.equal(b.hello.target, 'rpc')
})

test('a hello proof carries the requested signing time', async () => {
  const key = await auth.generateAuthKey()
  const time = Date.UTC(2030, 0, 1, 0, 0, 30, 999)
  const prepared = await prepareSession(
    auth.requestSigner(key),
    fixture.context,
    'rpc',
    time,
  )
  const body = await decodeFirst(prepared.message)
  const proof = prepared.message.subarray(body.length)
  assert.equal(
    BigInt((await decodeFirst(proof)).value.commitment.timestamp),
    BigInt(Math.floor(time / 1000)),
  )
  assert.equal(
    await auth.verifyRequestProof(
      proof,
      fixture.context,
      prepared.message.subarray(0, body.length),
    ),
    auth.normalizeDer64(key.pubkeyPem),
  )
})

test('a native retry answer yields the server clock and nothing else does', async () => {
  const retry = Uint8Array.from(
    Buffer.from(
      'a36870726f746f636f6c7153746172742d5250432d53657373696f6e' +
        '6776657273696f6e016568656c6c6fa264726f6c6565726574727969' +
        '74696d657374616d701a713fb300',
      'hex',
    ),
  )
  assert.equal(await retryTime(retry), 1_900_000_000_000)
  assert.equal(
    await retryTime(concatenate(retry, new Uint8Array([0]))),
    undefined,
  )
  assert.equal(await retryTime(server), undefined)
  assert.equal(await retryTime(client), undefined)
  const body = { ...(await decodeFirst(retry)).value }
  assert.equal(
    await retryTime(encode({ ...body, hello: { ...body.hello, extra: 1 } })),
    undefined,
  )
  assert.equal(
    await retryTime(encode({ ...body, protocol: 'Other' })),
    undefined,
  )
})

test('independent WebCrypto golden matches both directional records and full captured hellos', async context => {
  const prepared = await frozenClient(context)
  assert.deepEqual(prepared.message, client)
  const admitted = await prepared.finish(server)
  assert.equal(admitted.peer, fixture.serverIdentity)
  assert.equal(admitted.temporary, false)
  assert.deepEqual(
    await admitted.cipher.write(encoder.encode(fixture.clientRecord.plaintext)),
    bytes(fixture.clientRecord.ciphertext),
  )
  assert.equal(
    new TextDecoder().decode(
      await admitted.cipher.read(bytes(fixture.serverRecord.ciphertext)),
    ),
    fixture.serverRecord.plaintext,
  )
  await assert.rejects(prepared.finish(server), /already consumed/)
})

test('ordinary HTTP without crypto.subtle retains the independent handshake and both directional records', async context => {
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, 'crypto')
  const random = crypto.getRandomValues.bind(crypto)
  Object.defineProperty(globalThis, 'crypto', {
    value: { getRandomValues: random },
    configurable: true,
  })
  context.after(() => Object.defineProperty(globalThis, 'crypto', descriptor))
  const prepared = await frozenClient(context)
  const admitted = await prepared.finish(server)
  assert.equal(admitted.peer, fixture.serverIdentity)
  assert.deepEqual(
    await admitted.cipher.write(encoder.encode(fixture.clientRecord.plaintext)),
    bytes(fixture.clientRecord.ciphertext),
  )
  assert.equal(
    new TextDecoder().decode(
      await admitted.cipher.read(bytes(fixture.serverRecord.ciphertext)),
    ),
    fixture.serverRecord.plaintext,
  )
  const signer = await auth.generateEphemeralRequestSigner()
  const proof = await signer.sign('startup.local', payload)
  assert.match(
    await auth.verifyRequestProof(proof, 'startup.local', payload),
    /^MCow/,
  )
})

test('temporary identity scope comes from the signed server hello', async context => {
  const body = (await decodeFirst(server)).value
  body.hello.temporary = true
  const wire = await signedResponse(encode(body))
  const prepared = await frozenClient(context)
  const admitted = await prepared.finish(wire)
  assert.equal(admitted.temporary, true)
  const changed = (await decodeFirst(wire)).value
  changed.hello.temporary = false
  const tampered = concatenate(
    encode(changed),
    wire.subarray((await decodeFirst(wire)).length),
  )
  const another = await frozenClient(context)
  await assert.rejects(another.finish(tampered))
})

test('admission rejects unknown signed fields and invalid binary shapes', async context => {
  const signer = auth.requestSigner(await auth.generateAuthKey())
  for (const mode of ['unknown', 'text', 'array', 'length', 'temporary']) {
    const prepared = await frozenClient(context)
    const body = (await decodeFirst(server)).value
    if (mode === 'unknown') body.hello.unexpected = true
    if (mode === 'temporary') body.hello.temporary = 'true'
    if (mode === 'text') body.hello.ephemeral = fixture.serverPublic
    if (mode === 'array') body.hello.ephemeral = [...body.hello.ephemeral]
    if (mode === 'length')
      body.hello.ephemeral = body.hello.ephemeral.subarray(1)
    const captured = encode(body)
    await assert.rejects(
      prepared.finish(
        concatenate(captured, await signer.sign(fixture.context, captured)),
      ),
    )
    context.mock.restoreAll()
  }
})

test('admission rejects a correctly signed response with normalized typed-byte key material', async context => {
  const prepared = await frozenClient(context)
  const decoded = await decodeFirst(server)
  const captured = server.subarray(0, decoded.length)
  const field = encode(decoded.value.hello.ephemeral)
  const offset = Buffer.from(captured).indexOf(field)
  assert.ok(offset >= 0)
  const tagged = concatenate(
    captured.subarray(0, offset),
    new Uint8Array([0xd8, 0x40]),
    captured.subarray(offset),
  )
  await assert.rejects(prepared.finish(await signedResponse(tagged)))
})

test('admission requires exactly two CBOR objects and rejects the JSON predecessor', async context => {
  for (const response of [
    concatenate(server, encode(null)),
    server.subarray(0, server.length - 1),
    encoder.encode(
      JSON.stringify({ payload: fixture.server, proof: 'unused' }),
    ),
    concatenate(encoder.encode('\uFEFF'), server),
  ]) {
    const prepared = await frozenClient(context)
    await assert.rejects(prepared.finish(response))
    context.mock.restoreAll()
  }
})

test('a valid signed response for another captured client hello cannot admit this connection', async context => {
  const body = (await decodeFirst(server.slice())).value
  body.hello.clientHello[0] ^= 1
  const response = await signedResponse(encode(body))
  const prepared = await frozenClient(context)
  await assert.rejects(prepared.finish(response), /does not bind/)
})

test('tamper, replay and reflected directional ciphertext fail authentication', async context => {
  for (const mode of ['tamper', 'replay', 'reflection']) {
    const prepared = await frozenClient(context)
    const { cipher } = await prepared.finish(server)
    const record = bytes(fixture.serverRecord.ciphertext)
    if (mode === 'tamper') record[0] ^= 1
    if (mode === 'replay') await cipher.read(record)
    const input =
      mode === 'reflection'
        ? await cipher.write(encoder.encode('reflection'))
        : record
    await assert.rejects(cipher.read(input))
    context.mock.restoreAll()
  }
})
