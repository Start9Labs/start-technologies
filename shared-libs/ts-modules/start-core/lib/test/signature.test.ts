import { webcrypto } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { decodeFirst, encode } from '../auth/cbor'
import type { BinaryRequestProof } from '../osBindings/session'
import {
  ed25519PublicKeyDer64,
  normalizeDer64,
  parseEd25519PublicKey,
  signRequest,
  requestSigner,
  verifyRequestProof,
  verifyingKeyFingerprint,
  generateAuthKey,
  generateEphemeralRequestSigner,
} from '../auth/signature'

const der64 = 'MCowBQYDK2VwAyEAgTl3Dqh9F19Wo1Rmw0x+zMuNipG07jeiXfYPW4/Js5Q='
const pem = `-----BEGIN PUBLIC KEY-----\n${der64}\n-----END PUBLIC KEY-----\n`

beforeAll(() => {
  Object.defineProperty(globalThis, 'crypto', { value: webcrypto })
})

test('ephemeral possession signs and verifies without crypto.subtle or exposing enrollment material', async () => {
  Object.defineProperty(globalThis, 'crypto', {
    value: { getRandomValues: webcrypto.getRandomValues.bind(webcrypto) },
  })
  try {
    const signer = await generateEphemeralRequestSigner()
    expect(Object.keys(signer)).toEqual(['sign'])
    const body = new TextEncoder().encode('captured body')
    const proof = await signer.sign('startup.local', body)
    const peer = await verifyRequestProof(proof, 'startup.local', body)
    expect(await verifyingKeyFingerprint(peer)).toMatch(/^SHA256:[0-9a-f]{64}$/)
    const again = await signer.sign('startup.local', body)
    expect(await verifyRequestProof(again, 'startup.local', body)).toBe(peer)
    await expect(
      verifyRequestProof(proof, 'other.local', body),
    ).rejects.toThrow()
  } finally {
    Object.defineProperty(globalThis, 'crypto', { value: webcrypto })
  }
})

test('request proof verification rejects a normalized typed-byte tag', async () => {
  const provider = await generateEphemeralRequestSigner()
  const body = new TextEncoder().encode('captured body')
  const proof = await provider.sign('server.test', body)
  const parsed = (await decodeFirst(proof)).value as BinaryRequestProof
  const field = encode(parsed.signature)
  const offset = Buffer.from(proof).indexOf(field)
  expect(offset).toBeGreaterThanOrEqual(0)
  const tagged = new Uint8Array([
    ...proof.subarray(0, offset),
    0xd8,
    0x40,
    ...proof.subarray(offset),
  ])
  await expect(
    verifyRequestProof(tagged, 'server.test', body),
  ).rejects.toThrow()
})

test('DER64 normalization preserves standard Base64 and padding', () => {
  expect(normalizeDer64(pem)).toBe(der64)
  expect(normalizeDer64(pem.replace(/\n/g, '\r\n'))).toBe(der64)
  expect(normalizeDer64(der64)).toBe(der64)
  expect(normalizeDer64(der64.slice(0, -1))).toBe(der64)
  for (const input of [
    'MCowBQYDK2VwAyEAgTl3Dqh9F19Wo1Rmw0x-zMuNipG07jeiXfYPW4_Js5Q',
    'MCowBQYDK2VwAyEAgTl3Dqh9F19Wo1Rmw0x-zMuNipG07jeiXfYPW4_Js5Q=',
  ]) {
    expect(normalizeDer64(input)).toBe(der64)
  }
})

test('public-key decoding rejects off-curve points but retains backend ZIP-215 encodings', () => {
  const offCurve = Uint8Array.from([2, ...new Array(31).fill(0)])
  expect(() => parseEd25519PublicKey(ed25519PublicKeyDer64(offCurve))).toThrow(
    'Public key is not a curve point',
  )
  for (const key of [
    Uint8Array.from([1, ...new Array(31).fill(0)]),
    Uint8Array.from([238, ...new Array(30).fill(255), 127]),
  ]) {
    expect(parseEd25519PublicKey(ed25519PublicKeyDer64(key))).toEqual(key)
  }
  expect(parseEd25519PublicKey(pem)).toEqual(parseEd25519PublicKey(der64))
})

test('HTTP signature headers retain their base64url envelope', async () => {
  const privateKey = await crypto.subtle.importKey(
    'pkcs8',
    new Uint8Array(
      Buffer.from(
        'MC4CAQAwBQYDK2VwBCIEIAICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgIC',
        'base64',
      ),
    ),
    'Ed25519',
    false,
    ['sign'],
  )
  const header = await signRequest(
    { privateKey, pubkeyPem: pem },
    'start-9.local',
    new TextEncoder().encode('{}'),
  )
  const query = new URLSearchParams(header)
  expect(query.get('signer')).toBe(
    'MCowBQYDK2VwAyEAgTl3Dqh9F19Wo1Rmw0x-zMuNipG07jeiXfYPW4_Js5Q',
  )
  for (const field of ['signer', 'signature', 'blake3']) {
    expect(query.get(field)).toMatch(/^[A-Za-z0-9_-]+$/)
  }
})

test('independent native-consumed WebCrypto proofs preserve full-u64 commitments', async () => {
  const fixture = JSON.parse(
    readFileSync(
      resolve(
        __dirname,
        '../../../../crates/start-core/testdata/signed-session/webcrypto.json',
      ),
      'utf8',
    ),
  )
  for (const side of ['client', 'server']) {
    const wire = new Uint8Array(Buffer.from(fixture[side], 'base64'))
    const body = await decodeFirst(wire)
    const payload = wire.subarray(0, body.length)
    const proof = wire.subarray(body.length)
    const decoded = (await decodeFirst(proof)).value as BinaryRequestProof
    await expect(
      verifyRequestProof(proof, fixture.context, payload),
    ).resolves.toBe(fixture[side + 'Identity'])
    await expect(
      verifyRequestProof(proof, 'another.test', payload),
    ).rejects.toThrow()
    await expect(
      verifyRequestProof(
        proof,
        fixture.context,
        new Uint8Array([...payload, 0]),
      ),
    ).rejects.toThrow()
    expect(BigInt(decoded.commitment.nonce)).toBeGreaterThan(
      BigInt(Number.MAX_SAFE_INTEGER),
    )
    for (const [name, value] of [
      ['nonce', 18446744073709551616n],
      ['timestamp', 9223372036854775808n],
      ['size', -1n],
    ] as const) {
      const changed = encode({
        ...decoded,
        commitment: { ...decoded.commitment, [name]: value },
      })
      await expect(
        verifyRequestProof(changed, fixture.context, payload),
      ).rejects.toThrow()
    }
    const duplicate = new Uint8Array([
      ...proof,
      ...encode('signature'),
      ...encode(decoded.signature),
    ])
    duplicate[0] = 0xa4
    await expect(
      verifyRequestProof(duplicate, fixture.context, payload),
    ).rejects.toThrow('Duplicate')
    await expect(
      verifyRequestProof(
        new Uint8Array([...proof, 0xf6]),
        fixture.context,
        payload,
      ),
    ).rejects.toThrow('Trailing')
    await expect(
      verifyRequestProof(
        encode({ ...decoded, unexpected: 0 }),
        fixture.context,
        payload,
      ),
    ).rejects.toThrow()
  }
})

test('generic opaque signer shares commitment serialization without crossing the HTTP signature domain', async () => {
  const key = await generateAuthKey()
  const provider = requestSigner(key)
  const bytes = new TextEncoder().encode(
    '\uFEFF{"id":18446744073709551615,"value":1.23000000000000001}',
  )
  const proof = await provider.sign('server.test', bytes)
  expect(key.privateKey.extractable).toBe(false)
  await expect(verifyRequestProof(proof, 'server.test', bytes)).resolves.toBe(
    normalizeDer64(key.pubkeyPem),
  )
  const first = (await decodeFirst(proof)).value as BinaryRequestProof
  const second = (await decodeFirst(await provider.sign('server.test', bytes)))
    .value as BinaryRequestProof
  expect(BigInt(first.commitment.nonce)).not.toBe(
    BigInt(second.commitment.nonce),
  )
  await expect(
    verifyRequestProof(proof, 'server.test', bytes.subarray(3)),
  ).rejects.toThrow()
  const header = await signRequest(key, 'server.test', bytes)
  await expect(
    verifyRequestProof(new TextEncoder().encode(header), 'server.test', bytes),
  ).rejects.toThrow()
})

test('generic identity fingerprints preserve existing raw-Ed25519 history', async () => {
  const key = ed25519PublicKeyDer64(
    new Uint8Array(
      Buffer.from(
        'd54207da194977dcf46adbfec2bc2e75b52d5a8a42184fedfdc00024f0e3e8da',
        'hex',
      ),
    ),
  )
  expect(await verifyingKeyFingerprint(key)).toBe(
    'SHA256:d7e1e084be213b01e506852af8198b9980b43e03749c67d82dcd0903f4001c77',
  )
})
