import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { auth } from '@start9labs/start-core'
import './typescript-loader.mjs'

const { prepareSession } = await import('../src/services/session-identity.ts')
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
const envelope = JSON.parse(new TextDecoder().decode(client))
const payload = bytes(envelope.payload)
const encoder = new TextEncoder()

async function frozenClient(context) {
  const secret = Uint8Array.from(Buffer.from(fixture.clientPrivate, 'hex'))
  const privateKey = await crypto.subtle.importKey(
    'pkcs8',
    new Uint8Array([
      ...Buffer.from('302e020100300506032b656e04220420', 'hex'),
      ...secret,
    ]),
    'X25519',
    false,
    ['deriveBits'],
  )
  const publicKey = await crypto.subtle.importKey(
    'raw',
    bytes(fixture.clientPublic),
    'X25519',
    true,
    [],
  )
  context.mock.method(crypto.subtle, 'generateKey', async () => ({
    privateKey,
    publicKey,
  }))
  return prepareSession(
    {
      sign: async (name, body) => {
        assert.equal(name, fixture.context)
        assert.deepEqual(body, payload)
        return envelope.proof
      },
    },
    fixture.context,
    'rpc',
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
  const decode = prepared => {
    const wire = JSON.parse(new TextDecoder().decode(prepared.message))
    const payload = bytes(wire.payload)
    assert.equal(
      auth.verifyRequestProof(wire.proof, fixture.context, payload),
      auth.normalizeDer64(key.pubkeyPem),
    )
    return {
      hello: JSON.parse(new TextDecoder().decode(payload)).hello,
      nonce: new URLSearchParams(wire.proof).get('nonce'),
    }
  }
  const a = decode(one),
    b = decode(two)
  assert.notEqual(a.hello.ephemeral, b.hello.ephemeral)
  assert.notEqual(a.nonce, b.nonce)
  assert.deepEqual(a.hello.target, {
    continuation: 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA',
  })
  assert.equal(b.hello.target, 'rpc')
})

test('independent WebCrypto golden matches both directional records and full captured hellos', async context => {
  const prepared = await frozenClient(context)
  assert.deepEqual(prepared.message, client)
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
  await assert.rejects(prepared.finish(server), /already consumed/)
})

test('admission rejects unknown envelope fields and preserves BOM rejection', async context => {
  for (const mode of ['unknown', 'bom']) {
    const prepared = await frozenClient(context)
    const envelope = JSON.parse(new TextDecoder().decode(server))
    if (mode === 'unknown') envelope.unexpected = true
    const response = encoder.encode(
      (mode === 'bom' ? '\uFEFF' : '') + JSON.stringify(envelope),
    )
    await assert.rejects(prepared.finish(response))
    context.mock.restoreAll()
  }
})

test('native unpadded envelope Base64 is accepted without changing captured transcript bytes', async context => {
  const prepared = await frozenClient(context)
  const envelope = JSON.parse(new TextDecoder().decode(server))
  envelope.payload = envelope.payload.replace(/=+$/, '')
  const admitted = await prepared.finish(
    encoder.encode(JSON.stringify(envelope)),
  )
  assert.equal(admitted.peer, fixture.serverIdentity)
})

test('a signed response for another captured client hello cannot admit this connection', async context => {
  const prepared = await frozenClient(context)
  const changed = JSON.parse(new TextDecoder().decode(server))
  changed.proof = envelope.proof
  await assert.rejects(prepared.finish(encoder.encode(JSON.stringify(changed))))
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
