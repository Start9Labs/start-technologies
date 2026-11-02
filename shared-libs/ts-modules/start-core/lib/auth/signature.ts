import { blake3 } from '@noble/hashes/blake3'
import { concatBytes, hexToBytes } from '@noble/hashes/utils'

/**
 * Client side of the server's signature auth (`X-Start-Auth-Sig`).
 *
 * A request is authorized by signing, with an enrolled Ed25519 key, a
 * message of:
 *
 *   "Start-Auth-Sig v2\0" || len(context) || context || 3 || timestamp || nonce || size || blake3(body)
 *
 * — a fixed protocol tag (cross-protocol separation), the server identity
 * (hostname/IP/domain) the signature is bound to, prefixed with its length as
 * one byte, the request commitment's type, and the commitment. The
 * header value is the query-encoded commitment, tagged with its type, plus the signer's public key
 * and the signature, each as bare base64 DER (no PEM armor).
 *
 * Keys are WebCrypto and non-extractable: a compromised page can sign while
 * it lives, but can never read the key out for offline use. Requires a
 * secure context and Ed25519 WebCrypto support (all evergreen browsers).
 */

const REQUEST_AUTH_TAG = new TextEncoder().encode('Start-Auth-Sig v2\0')

// The server's `SignedCommitment::TYPE_ID` for `RequestCommitment`.
const REQUEST_COMMITMENT_TYPE = 3

export const AUTH_SIG_HEADER = 'X-Start-Auth-Sig'

// DER prefix of the server's SIGNATURE document: SEQUENCE(SEQUENCE(OID 1.3.101.112), OCTET STRING)
const SIGNATURE_PREFIX = hexToBytes('3049300506032b65700440')

export interface AuthKey {
  /** Non-extractable Ed25519 signing key. */
  privateKey: CryptoKey
  /** PEM-encoded public key, as sent in `LoginParams.pubkey`. */
  pubkeyPem: string
}

export async function generateAuthKey(): Promise<AuthKey> {
  const { privateKey, publicKey } = (await crypto.subtle.generateKey(
    'Ed25519',
    false,
    ['sign', 'verify'],
  )) as CryptoKeyPair
  const spki = new Uint8Array(await crypto.subtle.exportKey('spki', publicKey))
  return { privateKey, pubkeyPem: derToPem('PUBLIC KEY', spki) }
}

export async function signRequest(
  key: AuthKey,
  context: string,
  body: Uint8Array,
): Promise<string> {
  const timestamp = BigInt(Math.floor(Date.now() / 1000))
  const nonce = crypto.getRandomValues(new BigUint64Array(1))[0]
  const size = BigInt(body.length)
  const hash = blake3(body)
  const contextBytes = new TextEncoder().encode(context)
  if (contextBytes.length > 255) {
    throw new Error('Signing context is longer than 255 bytes')
  }

  const typeAt = REQUEST_AUTH_TAG.length + 1 + contextBytes.length
  const commitmentStart = typeAt + 1
  const message = new Uint8Array(commitmentStart + 56)
  message.set(REQUEST_AUTH_TAG, 0)
  message[REQUEST_AUTH_TAG.length] = contextBytes.length
  message.set(contextBytes, REQUEST_AUTH_TAG.length + 1)
  message[typeAt] = REQUEST_COMMITMENT_TYPE
  const view = new DataView(message.buffer, commitmentStart, 24)
  view.setBigInt64(0, timestamp, false)
  view.setBigUint64(8, nonce, false)
  view.setBigUint64(16, size, false)
  message.set(hash, commitmentStart + 24)

  const signature = new Uint8Array(
    await crypto.subtle.sign('Ed25519', key.privateKey, message),
  )

  const params = new URLSearchParams()
  params.set('commitment', 'request')
  params.set('timestamp', timestamp.toString())
  params.set('nonce', nonce.toString())
  params.set('size', size.toString())
  params.set('blake3', base64UrlEncode(hash))
  params.set(
    'signer',
    base64UrlEncode(base64ToBytes(normalizeDer64(key.pubkeyPem))),
  )
  params.set(
    'signature',
    base64UrlEncode(concatBytes(SIGNATURE_PREFIX, signature)),
  )
  return params.toString()
}

function derToPem(label: string, der: Uint8Array): string {
  const body = bytesToBase64(der)
  const lines: string[] = []
  for (let i = 0; i < body.length; i += 64) {
    lines.push(body.slice(i, i + 64))
  }
  return `-----BEGIN ${label}-----\n${lines.join('\n')}\n-----END ${label}-----\n`
}

/** Normalizes PEM and Base64 DER to padded standard Base64. */
export function normalizeDer64(value: string): string {
  const body = value
    .replace(/-----[^-]*-----|\s/g, '')
    .replace(/-/g, '+')
    .replace(/_/g, '/')
  return body.padEnd(Math.ceil(body.length / 4) * 4, '=')
}

function base64UrlEncode(bytes: Uint8Array): string {
  return bytesToBase64(bytes)
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '')
}

export function bytesToBase64(bytes: Uint8Array): string {
  let binary = ''
  for (const b of bytes) {
    binary += String.fromCharCode(b)
  }
  return btoa(binary)
}

export function base64ToBytes(b64: string): Uint8Array {
  const binary = atob(b64)
  const out = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) {
    out[i] = binary.charCodeAt(i)
  }
  return out
}
