import { ed25519 } from '@noble/curves/ed25519'
import { blake3 } from '@noble/hashes/blake3'

/**
 * Client side of the server's signature auth (`X-StartOS-Auth-Sig`).
 *
 * A request is authorized by signing, with an enrolled Ed25519 key (pure
 * Ed25519, WebCrypto-compatible), a message of:
 *
 *   "StartOS RPC Auth v1\0" || timestamp || nonce || size || blake3(body) || context
 *
 * — a fixed protocol tag (cross-protocol separation), the request commitment,
 * and the server identity (hostname/IP/domain) the signature is bound to. The
 * header value is the query-encoded commitment plus the signer's PEM public
 * key and the PEM-wrapped signature.
 */

const REQUEST_AUTH_TAG = new TextEncoder().encode('StartOS RPC Auth v1\0')

export const AUTH_SIG_HEADER = 'X-StartOS-Auth-Sig'

// DER prefix of an Ed25519 SubjectPublicKeyInfo: SEQUENCE(SEQUENCE(OID 1.3.101.112), BIT STRING)
const SPKI_PREFIX = hexToBytes('302a300506032b6570032100')
// DER prefix of the server's SIGNATURE document: SEQUENCE(SEQUENCE(OID 1.3.101.112), OCTET STRING)
const SIGNATURE_PREFIX = hexToBytes('3049300506032b65700440')

export interface AuthKey {
  /** Raw 32-byte Ed25519 secret key. */
  secretKey: Uint8Array
  /** PEM-encoded public key, as sent in `LoginParams.pubkey` and the header. */
  pubkeyPem: string
}

export function generateAuthKey(): AuthKey {
  const secretKey = ed25519.utils.randomSecretKey()
  return { secretKey, pubkeyPem: pubkeyToPem(ed25519.getPublicKey(secretKey)) }
}

export function pubkeyToPem(publicKey: Uint8Array): string {
  return derToPem('PUBLIC KEY', concatBytes(SPKI_PREFIX, publicKey))
}

export function signRequest(
  key: AuthKey,
  context: string,
  body: Uint8Array,
): string {
  const timestamp = BigInt(Math.floor(Date.now() / 1000))
  const nonce = crypto.getRandomValues(new BigUint64Array(1))[0]
  const size = BigInt(body.length)
  const hash = blake3(body)

  const message = new Uint8Array(REQUEST_AUTH_TAG.length + 56 + context.length)
  message.set(REQUEST_AUTH_TAG, 0)
  const view = new DataView(message.buffer, REQUEST_AUTH_TAG.length, 24)
  view.setBigInt64(0, timestamp, false)
  view.setBigUint64(8, nonce, false)
  view.setBigUint64(16, size, false)
  message.set(hash, REQUEST_AUTH_TAG.length + 24)
  message.set(new TextEncoder().encode(context), REQUEST_AUTH_TAG.length + 56)

  const signature = ed25519.sign(message, key.secretKey)

  const params = new URLSearchParams()
  params.set('timestamp', timestamp.toString())
  params.set('nonce', nonce.toString())
  params.set('size', size.toString())
  params.set('blake3', base64Encode(hash))
  params.set('signer', key.pubkeyPem)
  params.set(
    'signature',
    derToPem('SIGNATURE', concatBytes(SIGNATURE_PREFIX, signature)),
  )
  return params.toString()
}

function derToPem(label: string, der: Uint8Array): string {
  const body = base64EncodePadded(der)
  const lines: string[] = []
  for (let i = 0; i < body.length; i += 64) {
    lines.push(body.slice(i, i + 64))
  }
  return `-----BEGIN ${label}-----\n${lines.join('\n')}\n-----END ${label}-----\n`
}

function base64Encode(bytes: Uint8Array): string {
  return base64EncodePadded(bytes).replace(/=+$/, '')
}

function base64EncodePadded(bytes: Uint8Array): string {
  let binary = ''
  for (const b of bytes) {
    binary += String.fromCharCode(b)
  }
  return btoa(binary)
}

function concatBytes(a: Uint8Array, b: Uint8Array): Uint8Array {
  const out = new Uint8Array(a.length + b.length)
  out.set(a, 0)
  out.set(b, a.length)
  return out
}

function hexToBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2)
  for (let i = 0; i < out.length; i++) {
    out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16)
  }
  return out
}
