import { webcrypto } from 'node:crypto'
import { p256 } from '@noble/curves/p256'
import { sha256 } from '@noble/hashes/sha256'
import { concatBytes } from '@noble/hashes/utils'
import {
  base64ToBytes,
  createPasskey,
  normalizeDer64,
  signRequestWithPasskey,
} from '../auth/signature'

const secret = new Uint8Array(32).fill(3)
const spkiPrefix = Buffer.from(
  '3059301306072a8648ce3d020106082a8648ce3d030107034200',
  'hex',
)
const spki = concatBytes(spkiPrefix, p256.getPublicKey(secret, false))
const credentialId = new Uint8Array([1, 2, 3, 4])
const flags = { userPresent: 0x01, userVerified: 0x04 }

function base64Url(bytes: Uint8Array): string {
  return Buffer.from(bytes).toString('base64url')
}

function softwareAuthenticator(origin: string) {
  return {
    create: async () => ({
      rawId: credentialId.buffer,
      response: { getPublicKey: () => spki.buffer },
    }),
    get: async ({ publicKey }: CredentialRequestOptions) => {
      const challenge = new Uint8Array(publicKey!.challenge as ArrayBuffer)
      const clientDataJSON = new TextEncoder().encode(
        JSON.stringify({
          type: 'webauthn.get',
          challenge: base64Url(challenge),
          origin,
          crossOrigin: false,
        }),
      )
      const authenticatorData = concatBytes(
        sha256(new TextEncoder().encode(new URL(origin).hostname)),
        new Uint8Array([flags.userPresent | flags.userVerified, 0, 0, 0, 1]),
      )
      const signature = p256
        .sign(
          sha256(concatBytes(authenticatorData, sha256(clientDataJSON))),
          secret,
        )
        .toDERRawBytes()
      return {
        rawId: credentialId.buffer,
        response: {
          authenticatorData: authenticatorData.buffer,
          clientDataJSON: clientDataJSON.buffer,
          signature: signature.buffer,
        },
      }
    },
  }
}

function readTlv(bytes: Uint8Array, tag: number) {
  expect(bytes[0]).toBe(tag)
  let length = bytes[1]
  let offset = 2
  if (length & 0x80) {
    const width = length & 0x7f
    length = 0
    for (let i = 0; i < width; i++) length = (length << 8) | bytes[2 + i]
    offset += width
  }
  return {
    content: bytes.subarray(offset, offset + length),
    rest: bytes.subarray(offset + length),
  }
}

beforeAll(() => {
  Object.defineProperty(globalThis, 'crypto', { value: webcrypto })
  Object.defineProperty(globalThis, 'navigator', {
    value: { credentials: softwareAuthenticator('https://start-9.local') },
  })
})

test('a passkey signs a request header the authenticator assertion verifies', async () => {
  const passkey = await createPasskey(
    {
      id: new Uint8Array([9]),
      name: 'admin',
      displayName: 'Admin',
    },
    'StartOS',
  )
  expect(passkey.credentialId).toBe(base64Url(credentialId))
  const body = new TextEncoder().encode(
    '{"jsonrpc":"2.0","id":1,"method":"server.echo","params":{}}',
  )
  const other = {
    credentialId: base64Url(new Uint8Array([5, 6])),
    pubkeyPem: base64Url(p256.getPublicKey(new Uint8Array(32).fill(4))),
  }
  const header = new URLSearchParams(
    await signRequestWithPasskey([other, passkey], 'start-9.local', body),
  )
  expect(header.get('signer')).toBe(base64Url(spki))

  const encoded = base64ToBytes(normalizeDer64(header.get('signature')!))
  const outer = readTlv(encoded, 0x30)
  expect(outer.rest.length).toBe(0)
  const algorithm = readTlv(outer.content, 0x30)
  const octets = readTlv(algorithm.rest, 0x04)
  const assertion = readTlv(octets.content, 0x30)
  const authenticatorData = readTlv(assertion.content, 0x04)
  const clientDataJSON = readTlv(authenticatorData.rest, 0x04)
  const signature = readTlv(clientDataJSON.rest, 0x04)
  expect(signature.rest.length).toBe(0)
  expect(
    p256.verify(
      signature.content,
      sha256(
        concatBytes(authenticatorData.content, sha256(clientDataJSON.content)),
      ),
      p256.getPublicKey(secret),
    ),
  ).toBe(true)
  expect(
    JSON.parse(new TextDecoder().decode(clientDataJSON.content)).type,
  ).toBe('webauthn.get')
})
