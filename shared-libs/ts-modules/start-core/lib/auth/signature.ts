import { ed25519, ed25519ph } from '@noble/curves/ed25519'
import { blake3 } from '@noble/hashes/blake3'
import { bytesToHex, concatBytes } from '@noble/hashes/utils'
import { SIGNATURE_PROFILE } from '../osBindings/signature-profile'

const REQUEST_AUTH_TAG = new Uint8Array(SIGNATURE_PROFILE.http.tag)
const COMMITMENT_TAG = new Uint8Array(SIGNATURE_PROFILE.commitmentTag)
const ED25519_SPKI_PREFIX = new Uint8Array(SIGNATURE_PROFILE.ed25519.keyPrefix)

export const AUTH_SIG_HEADER = SIGNATURE_PROFILE.http.header
export const SIGNATURE_PREFIX = new Uint8Array(
  SIGNATURE_PROFILE.ed25519.signaturePrefix,
)

export interface AuthKey {
  /** Non-extractable Ed25519 signing key. */
  privateKey: CryptoKey
  /** PEM-encoded public key, as sent in `LoginParams.pubkey`. */
  pubkeyPem: string
}

/** Produces a fresh generic request proof without exposing private key material. */
export interface RequestSigner {
  sign(context: string, body: Uint8Array): Promise<string>
}

/** Uses the enrolled opaque WebCrypto identity for generic request commitments. */
export function requestSigner(key: AuthKey): RequestSigner {
  return {
    sign: (context, body) =>
      signTaggedRequest(key, context, body, COMMITMENT_TAG),
  }
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
  return signTaggedRequest(key, context, body, REQUEST_AUTH_TAG)
}

function requestCommitment(
  timestamp: bigint,
  nonce: bigint,
  size: bigint,
  hash: Uint8Array,
) {
  if (
    timestamp < -(1n << 63n) ||
    timestamp >= 1n << 63n ||
    nonce < 0n ||
    nonce >= 1n << 64n ||
    size < 0n ||
    size >= 1n << 64n ||
    hash.length !== 32
  )
    throw new Error('Invalid request commitment')
  const bytes = new Uint8Array(56)
  const view = new DataView(bytes.buffer)
  view.setBigInt64(0, timestamp)
  view.setBigUint64(8, nonce)
  view.setBigUint64(16, size)
  bytes.set(hash, 24)
  return bytes
}

async function signTaggedRequest(
  key: AuthKey,
  context: string,
  body: Uint8Array,
  tag: Uint8Array,
): Promise<string> {
  const timestamp = BigInt(Math.floor(Date.now() / 1000))
  const nonce = crypto.getRandomValues(new BigUint64Array(1))[0]
  const size = BigInt(body.length)
  const hash = blake3(body)
  const message = signingMessage(
    tag,
    context,
    SIGNATURE_PROFILE.requestType,
    requestCommitment(timestamp, nonce, size, hash),
  )
  const signature = new Uint8Array(
    await crypto.subtle.sign('Ed25519', key.privateKey, message),
  )
  return new URLSearchParams({
    commitment: 'request',
    timestamp: timestamp.toString(),
    nonce: nonce.toString(),
    size: size.toString(),
    blake3: base64UrlEncode(hash),
    signer: base64UrlEncode(base64ToBytes(normalizeDer64(key.pubkeyPem))),
    signature: base64UrlEncode(concatBytes(SIGNATURE_PREFIX, signature)),
  }).toString()
}

/** Verifies captured bytes with prime-order Ed25519/Ed25519ph points and returns canonical identity DER64. Replay policy remains with the receiver. */
export function verifyRequestProof(
  proof: string,
  context: string,
  body: Uint8Array,
): string {
  const query = new URLSearchParams(proof)
  const fields = new Map<string, string>()
  for (const [key, value] of query) {
    if (fields.has(key)) throw new Error('Duplicate request proof field')
    fields.set(key, value)
  }
  const field = (name: string) => {
    const value = fields.get(name)
    if (value === undefined) throw new Error('Missing request proof field')
    return value
  }
  const integer = (name: string) => {
    const value = field(name)
    if (!/^-?\d+$/.test(value)) throw new Error('Invalid request proof integer')
    return BigInt(value)
  }
  if (field('commitment') !== 'request')
    throw new Error('Unsupported request commitment')
  const hash = decodeBase64Url(field('blake3'))
  const size = integer('size')
  const commitment = requestCommitment(
    integer('timestamp'),
    integer('nonce'),
    size,
    hash,
  )
  if (
    size !== BigInt(body.length) ||
    !blake3(body).every((value, i) => value === hash[i])
  )
    throw new Error('Request proof body mismatch')
  const signer = bytesToBase64(decodeBase64Url(field('signer')))
  const key = parseEd25519PublicKey(signer)
  const signature = bytesToBase64(decodeBase64Url(field('signature')))
  const pure = SIGNATURE_PROFILE.ed25519.signaturePrefix
  const prehash = SIGNATURE_PROFILE.ed25519ph.signaturePrefix
  const encoded = base64ToBytes(signature)
  const matches = (prefix: readonly number[]) =>
    encoded.length === prefix.length + SIGNATURE_PROFILE.signatureBytes &&
    prefix.every((byte, i) => encoded[i] === byte)
  const offset = matches(pure)
    ? pure.length
    : matches(prehash)
      ? prehash.length
      : undefined
  if (offset === undefined)
    throw new Error('Unsupported request proof signature')
  const raw = encoded.subarray(offset)
  const publicPoint = ed25519.Point.fromHex(key)
  const noncePoint = ed25519.Point.fromHex(raw.subarray(0, 32))
  if (
    publicPoint.isSmallOrder() ||
    !publicPoint.isTorsionFree() ||
    noncePoint.isSmallOrder() ||
    !noncePoint.isTorsionFree()
  )
    throw new Error('Invalid request proof point')
  let valid: boolean
  if (matches(pure)) {
    valid = ed25519.verify(
      raw,
      signingMessage(
        COMMITMENT_TAG,
        context,
        SIGNATURE_PROFILE.requestType,
        commitment,
      ),
      key,
      { zip215: false },
    )
  } else if (matches(prehash)) {
    valid = ed25519ph.verify(raw, commitment, key, {
      context: signingContext(context),
      zip215: false,
    })
  } else {
    throw new Error('Unsupported request proof signature')
  }
  if (!valid) throw new Error('Invalid request proof signature')
  return signer
}

/** Stable fingerprint of a supported verifying key, independent of its signature scheme. */
export async function verifyingKeyFingerprint(
  serialized: string,
): Promise<string> {
  const raw = parseEd25519PublicKey(serialized)
  const profile = SIGNATURE_PROFILE.fingerprint
  const hash = await crypto.subtle.digest(profile.algorithm, raw.slice().buffer)
  return profile.prefix + bytesToHex(new Uint8Array(hash))
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
  for (let i = 0; i < body.length; i += 64) lines.push(body.slice(i, i + 64))
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

function decodeBase64Url(value: string): Uint8Array {
  if (!/^[A-Za-z0-9_-]+$/.test(value))
    throw new Error('Invalid Base64 URL encoding')
  const bytes = base64ToBytes(normalizeDer64(value))
  if (base64UrlEncode(bytes) !== value)
    throw new Error('Invalid Base64 URL encoding')
  return bytes
}

export function bytesToBase64(bytes: Uint8Array): string {
  let binary = ''
  for (const b of bytes) binary += String.fromCharCode(b)
  return btoa(binary)
}

export function base64ToBytes(b64: string): Uint8Array {
  const binary = atob(b64)
  const out = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i)
  return out
}

/** Canonical padded Base64 SPKI for a raw Ed25519 public key. */
export function ed25519PublicKeyDer64(raw32: Uint8Array): string {
  if (raw32.length !== SIGNATURE_PROFILE.ed25519.keyBytes)
    throw new Error('Invalid Ed25519 public key length')
  return bytesToBase64(concatBytes(ED25519_SPKI_PREFIX, raw32))
}

/** Decodes an on-curve Ed25519 SPKI, accepting DER64 and PEM serialization. */
export function parseEd25519PublicKey(serialized: string): Uint8Array {
  const key = parseDerPayload(
    serialized,
    ED25519_SPKI_PREFIX,
    SIGNATURE_PROFILE.ed25519.keyBytes,
  )
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
  )
    throw new Error('Invalid or unsupported DER encoding')
  return der.slice(prefix.length)
}
