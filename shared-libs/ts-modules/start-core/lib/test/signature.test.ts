import { webcrypto } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import {
  ed25519PublicKeyDer64,
  normalizeDer64,
  parseEd25519PublicKey,
  signRequest,
  requestSigner,
  verifyRequestProof,
  verifyingKeyFingerprint,
  generateAuthKey,
} from '../auth/signature'

const der64 = 'MCowBQYDK2VwAyEAgTl3Dqh9F19Wo1Rmw0x+zMuNipG07jeiXfYPW4/Js5Q='
const pem = `-----BEGIN PUBLIC KEY-----\n${der64}\n-----END PUBLIC KEY-----\n`

beforeAll(() => {
  Object.defineProperty(globalThis, 'crypto', { value: webcrypto })
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

test('independent native-consumed WebCrypto proofs preserve full-u64 commitments', () => {
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
    const envelope = JSON.parse(Buffer.from(fixture[side], 'base64').toString())
    const payload = new Uint8Array(Buffer.from(envelope.payload, 'base64'))
    expect(verifyRequestProof(envelope.proof, fixture.context, payload)).toBe(
      fixture[side + 'Identity'],
    )
    expect(() =>
      verifyRequestProof(envelope.proof, 'another.test', payload),
    ).toThrow()
    expect(() =>
      verifyRequestProof(
        envelope.proof,
        fixture.context,
        new Uint8Array([...payload, 0]),
      ),
    ).toThrow()
    const query = new URLSearchParams(envelope.proof)
    expect(BigInt(query.get('nonce')!)).toBeGreaterThan(
      BigInt(Number.MAX_SAFE_INTEGER),
    )
    for (const [name, value] of [
      ['nonce', '18446744073709551616'],
      ['timestamp', '9223372036854775808'],
      ['size', '-1'],
    ]) {
      const changed = new URLSearchParams(query)
      changed.set(name, value)
      expect(() =>
        verifyRequestProof(changed.toString(), fixture.context, payload),
      ).toThrow()
    }
    expect(() =>
      verifyRequestProof(envelope.proof + '&nonce=0', fixture.context, payload),
    ).toThrow('Duplicate')
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
  expect(verifyRequestProof(proof, 'server.test', bytes)).toBe(
    normalizeDer64(key.pubkeyPem),
  )
  expect(new URLSearchParams(proof).get('nonce')).not.toBe(
    new URLSearchParams(await provider.sign('server.test', bytes)).get('nonce'),
  )
  expect(() =>
    verifyRequestProof(proof, 'server.test', bytes.subarray(3)),
  ).toThrow()
  const header = await signRequest(key, 'server.test', bytes)
  expect(() => verifyRequestProof(header, 'server.test', bytes)).toThrow()
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
