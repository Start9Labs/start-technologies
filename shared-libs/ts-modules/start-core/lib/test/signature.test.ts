import { webcrypto } from 'node:crypto'
import {
  ed25519PublicKeyDer64,
  normalizeDer64,
  parseEd25519PublicKey,
  signRequest,
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
