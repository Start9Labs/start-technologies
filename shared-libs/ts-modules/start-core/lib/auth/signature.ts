import { ed25519 } from '@noble/curves/ed25519'
import { blake3 } from '@noble/hashes/blake3'
import { concatBytes, hexToBytes } from '@noble/hashes/utils'

/**
 * Ed25519 HTTP resource authentication using `Start-Auth-Sig v2` over raw bytes.
 * Generated WebCrypto private keys are non-extractable and require a secure context.
 */

const REQUEST_AUTH_TAG = new TextEncoder().encode('Start-Auth-Sig v2\0')
// The server's `SignedCommitment::TYPE_ID` for `RequestCommitment`.
const REQUEST_COMMITMENT_TYPE = 3

export const AUTH_SIG_HEADER = 'X-Start-Auth-Sig'

// DER prefix of the server's SIGNATURE document: SEQUENCE(SEQUENCE(OID 1.3.101.112), OCTET STRING)
export const SIGNATURE_PREFIX = hexToBytes('3049300506032b65700440')

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

/** Produces an HTTP authentication header for the complete resource body. */
export async function signRequest(
  key: AuthKey,
  context: string,
  body: Uint8Array,
): Promise<string> {
  const timestamp = BigInt(Math.floor(Date.now() / 1000))
  const nonce = crypto.getRandomValues(new BigUint64Array(1))[0]
  const size = BigInt(body.length)
  const hash = blake3(body)
  const commitment = new Uint8Array(56)
  const view = new DataView(commitment.buffer, 0, 24)
  view.setBigInt64(0, timestamp, false)
  view.setBigUint64(8, nonce, false)
  view.setBigUint64(16, size, false)
  commitment.set(hash, 24)
  const message = signingMessage(
    REQUEST_AUTH_TAG,
    context,
    REQUEST_COMMITMENT_TYPE,
    commitment,
  )

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

/** Encodes a signing context within the native one-byte length limit. */
export function signingContext(context: string): Uint8Array<ArrayBuffer> {
  const bytes = new TextEncoder().encode(context)
  if (bytes.length > 255)
    throw new Error('Signing context is longer than 255 bytes')
  return bytes
}

/** Frames a tagged commitment with its signing context and type identifier. */
export function signingMessage(
  tag: Uint8Array,
  context: string,
  type: number,
  commitment: Uint8Array,
): Uint8Array<ArrayBuffer> {
  const bytes = signingContext(context)
  return new Uint8Array([...tag, bytes.length, ...bytes, type, ...commitment])
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

const ED25519_SPKI_PREFIX = hexToBytes('302a300506032b6570032100')

/** Canonical padded Base64 SPKI for a raw Ed25519 public key. */
export function ed25519PublicKeyDer64(raw32: Uint8Array): string {
  if (raw32.length !== 32) throw new Error('Invalid Ed25519 public key length')
  return bytesToBase64(concatBytes(ED25519_SPKI_PREFIX, raw32))
}

/** Decodes an on-curve Ed25519 SPKI, accepting DER64 and PEM serialization. */
export function parseEd25519PublicKey(serialized: string): Uint8Array {
  const key = parseDerPayload(serialized, ED25519_SPKI_PREFIX, 32)
  // ZIP-215 decoding matches the backend's accepted curve points.
  if (!ed25519.utils.isValidPublicKey(key, true))
    throw new Error('Public key is not a curve point')
  return key
}

/** Decodes a fixed-layout DER value using native key/signature encodings. */
export function parseDerPayload(
  serialized: string,
  prefix: Uint8Array,
  length: number,
): Uint8Array {
  let body = serialized
  if (body.startsWith('-----BEGIN ')) {
    const pem =
      /^-----BEGIN ([^\r\n]+)-----\r?\n([\s\S]+)\r?\n-----END \1-----\r?\n?$/.exec(
        body,
      )
    if (!pem) throw new Error('Invalid PEM encoding')
    body = pem[2].replace(/\r?\n/g, '')
  }
  if (!/^[A-Za-z0-9+/_-]+={0,2}$/.test(body))
    throw new Error('Invalid Base64 DER encoding')
  const normalized = normalizeDer64(body)
  const der = base64ToBytes(normalized)
  if (
    bytesToBase64(der) !== normalized ||
    der.length !== prefix.length + length ||
    !prefix.every((byte, i) => der[i] === byte)
  ) {
    throw new Error('Invalid or unsupported DER encoding')
  }
  return der.slice(prefix.length)
}
