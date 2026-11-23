import { ed25519, ed25519ph } from '@noble/curves/ed25519'
import { p256 } from '@noble/curves/p256'
import { blake3 } from '@noble/hashes/blake3'
import { sha256 } from '@noble/hashes/sha256'
import { bytesToHex, concatBytes } from '@noble/hashes/utils'
import type { BinaryRequestProof } from '../osBindings/session'
import { SIGNATURE_PROFILE } from '../osBindings/signature-profile'

const REQUEST_AUTH_TAG = new Uint8Array(SIGNATURE_PROFILE.http.tag)
const COMMITMENT_TAG = new Uint8Array(SIGNATURE_PROFILE.commitmentTag)
const ED25519_SPKI_PREFIX = new Uint8Array(SIGNATURE_PROFILE.ed25519.keyPrefix)
const WEBAUTHN_ALGORITHM = new Uint8Array(SIGNATURE_PROFILE.webauthn.algorithm)
const COSE_ES256 = -7
const COSE_EDDSA = -8

/** Advertises registry support for P-256 keys and WebAuthn signatures. */
export const REGISTRY_SIGNATURE_HEADERS = {
  [SIGNATURE_PROFILE.registry.header]: SIGNATURE_PROFILE.registry.value,
} as const

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

/** A WebAuthn credential that signs StartOS request commitments. */
export interface Passkey {
  /** Base64url credential ID. */
  credentialId: string
  /** PEM-encoded P-256 or Ed25519 public key. */
  pubkeyPem: string
}

/** Signs captured body bytes and returns a binary CBOR request proof without exporting private keys. */
export interface RequestSigner {
  sign(context: string, body: Uint8Array): Promise<Uint8Array<ArrayBuffer>>
}

interface SigningIdentity {
  /** Returns a DER-encoded `AnySignature`, and the signer's SPKI DER unless the receiver resolves it. */
  sign(
    message: Uint8Array<ArrayBuffer>,
  ): Promise<{ signer?: Uint8Array; signature: Uint8Array }>
}

function webCryptoIdentity(key: AuthKey): SigningIdentity {
  const signer = base64ToBytes(normalizeDer64(key.pubkeyPem))
  return {
    sign: async message => ({
      signer,
      signature: concatBytes(
        SIGNATURE_PREFIX,
        new Uint8Array(
          await crypto.subtle.sign('Ed25519', key.privateKey, message),
        ),
      ),
    }),
  }
}

/**
 * Offers the given passkeys and names the chosen one's key. With none, offers the site's
 * discoverable passkeys and leaves the receiver to resolve the key from the credential ID.
 */
function passkeyIdentity(passkeys: readonly Passkey[]): SigningIdentity {
  const credentials = passkeys.map(passkey => ({
    id: new Uint8Array(base64ToBytes(normalizeDer64(passkey.credentialId))),
    signer: base64ToBytes(normalizeDer64(passkey.pubkeyPem)),
  }))
  return {
    sign: async message => {
      const credential = (await navigator.credentials.get({
        publicKey: {
          challenge: new Uint8Array(sha256(message)),
          allowCredentials: credentials.map(({ id }) => ({
            type: 'public-key' as const,
            id,
          })),
          userVerification: 'required',
        },
      })) as PublicKeyCredential | null
      if (!credential) throw new Error('Passkey signing was cancelled')
      const rawId = new Uint8Array(credential.rawId)
      const chosen = credentials.find(
        ({ id }) =>
          id.length === rawId.length &&
          id.every((byte, i) => byte === rawId[i]),
      )
      if (credentials.length && !chosen) throw new Error('Unknown passkey')
      const response = credential.response as AuthenticatorAssertionResponse
      return {
        signer: chosen?.signer,
        signature: webAuthnSignature(
          rawId,
          new Uint8Array(response.authenticatorData),
          new Uint8Array(response.clientDataJSON),
          new Uint8Array(response.signature),
        ),
      }
    },
  }
}

function derTlv(tag: number, content: Uint8Array): Uint8Array {
  const length: number[] = []
  for (let n = content.length; n > 0; n >>= 8) length.unshift(n & 0xff)
  const header =
    content.length < 0x80
      ? [tag, content.length]
      : [tag, 0x80 | length.length, ...length]
  return concatBytes(new Uint8Array(header), content)
}

function webAuthnSignature(
  credentialId: Uint8Array,
  authenticatorData: Uint8Array,
  clientDataJson: Uint8Array,
  signature: Uint8Array,
): Uint8Array {
  const assertion = derTlv(
    0x30,
    concatBytes(
      derTlv(0x04, credentialId),
      derTlv(0x04, authenticatorData),
      derTlv(0x04, clientDataJson),
      derTlv(0x04, signature),
    ),
  )
  return derTlv(0x30, concatBytes(WEBAUTHN_ALGORITHM, derTlv(0x04, assertion)))
}

function commitmentSigner(identity: SigningIdentity): RequestSigner {
  return {
    sign: async (context, body) => {
      const { encode } = await import('./cbor')
      return encode(
        await signTaggedRequest(identity, context, body, COMMITMENT_TAG),
      )
    },
  }
}

/** Uses the enrolled opaque WebCrypto identity for generic request commitments. */
export function requestSigner(key: AuthKey): RequestSigner {
  return commitmentSigner(webCryptoIdentity(key))
}

/** Generates an opaque Ed25519 possession signer without WebCrypto or persistence. */
export async function generateEphemeralRequestSigner(): Promise<RequestSigner> {
  const secret = ed25519.utils.randomPrivateKey()
  const signer = concatBytes(ED25519_SPKI_PREFIX, ed25519.getPublicKey(secret))
  return commitmentSigner({
    sign: async message => ({
      signer,
      signature: concatBytes(SIGNATURE_PREFIX, ed25519.sign(message, secret)),
    }),
  })
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

/** Registers a user-verified passkey for the current site. */
export async function createPasskey(
  user: PublicKeyCredentialUserEntity,
  rpName: string,
): Promise<Passkey> {
  const credential = (await navigator.credentials.create({
    publicKey: {
      challenge: crypto.getRandomValues(new Uint8Array(32)),
      rp: { name: rpName },
      user,
      pubKeyCredParams: [
        { type: 'public-key', alg: COSE_ES256 },
        { type: 'public-key', alg: COSE_EDDSA },
      ],
      authenticatorSelection: {
        residentKey: 'required',
        userVerification: 'required',
      },
      attestation: 'none',
    },
  })) as PublicKeyCredential | null
  if (!credential) throw new Error('Passkey registration was cancelled')
  const spki = (
    credential.response as AuthenticatorAttestationResponse
  ).getPublicKey()
  if (!spki) throw new Error('Passkey uses an unsupported algorithm')
  return {
    credentialId: base64UrlEncode(new Uint8Array(credential.rawId)),
    pubkeyPem: derToPem('PUBLIC KEY', new Uint8Array(spki)),
  }
}

/** Signs request commitments with a passkey, prompting once per request. */
export function passkeyRequestSigner(
  passkeys: readonly Passkey[] = [],
): RequestSigner {
  return commitmentSigner(passkeyIdentity(passkeys))
}

/** Produces an HTTP authentication header for the complete resource body. */
export async function signRequest(
  key: AuthKey,
  context: string,
  body: Uint8Array,
): Promise<string> {
  return authHeader(webCryptoIdentity(key), context, body)
}

/** Produces an HTTP authentication header, prompting for one of the given passkeys. */
export async function signRequestWithPasskey(
  passkeys: readonly Passkey[],
  context: string,
  body: Uint8Array,
): Promise<string> {
  return authHeader(passkeyIdentity(passkeys), context, body)
}

function requireSigner(signer: Uint8Array | null | undefined): Uint8Array {
  if (!signer) throw new Error('Request proof names no signer')
  return signer
}

async function authHeader(
  identity: SigningIdentity,
  context: string,
  body: Uint8Array,
): Promise<string> {
  const proof = await signTaggedRequest(
    identity,
    context,
    body,
    REQUEST_AUTH_TAG,
  )
  return new URLSearchParams({
    commitment: SIGNATURE_PROFILE.requestName,
    timestamp: proof.commitment.timestamp.toString(),
    nonce: proof.commitment.nonce.toString(),
    size: proof.commitment.size.toString(),
    blake3: base64UrlEncode(proof.commitment.blake3),
    signer: base64UrlEncode(requireSigner(proof.signer)),
    signature: base64UrlEncode(proof.signature),
  }).toString()
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
  identity: SigningIdentity,
  context: string,
  body: Uint8Array,
  tag: Uint8Array,
): Promise<BinaryRequestProof> {
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
  const { signer, signature } = await identity.sign(message)
  return {
    commitment: { timestamp, nonce, size, blake3: new Uint8Array(hash) },
    ...(signer ? { signer: new Uint8Array(signer) } : {}),
    signature: new Uint8Array(signature),
  }
}

/** Verifies captured bytes with prime-order Ed25519/Ed25519ph points and returns canonical identity DER64. Replay policy remains with the receiver. */
export async function verifyRequestProof(
  encodedProof: Uint8Array,
  context: string,
  body: Uint8Array,
): Promise<string> {
  const [{ decodeFirst }, schemas] = await Promise.all([
    import('./cbor'),
    import('../osBindings/session.schemas'),
  ])
  const decoded = await decodeFirst(encodedProof)
  if (decoded.length !== encodedProof.length)
    throw new Error('Trailing request proof data')
  const proof = schemas.BinaryRequestProof.extend({
    commitment: schemas.BinaryRequestCommitment.strict(),
  })
    .strict()
    .parse(decoded.value)
  const hash = proof.commitment.blake3
  const size = BigInt(proof.commitment.size)
  const commitment = requestCommitment(
    BigInt(proof.commitment.timestamp),
    BigInt(proof.commitment.nonce),
    size,
    hash,
  )
  if (
    size !== BigInt(body.length) ||
    !blake3(body).every((value, i) => value === hash[i])
  )
    throw new Error('Request proof body mismatch')
  const signer = bytesToBase64(requireSigner(proof.signer))
  const key = parseEd25519PublicKey(signer)
  const pure = SIGNATURE_PROFILE.ed25519.signaturePrefix
  const prehash = SIGNATURE_PROFILE.ed25519ph.signaturePrefix
  const encoded = proof.signature
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
  return profile.prefix + bytesToHex(sha256(raw))
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

/** PEM-armors DER bytes using 64-character lines. */
export function derToPem(label: string, der: Uint8Array): string {
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

/** Canonical SPKI DER64 for an Ed25519 or P-256 key, including compressed P-256 input. */
export function verifyingKeyDer64(serialized: string): string {
  const profile = SIGNATURE_PROFILE.p256
  for (const [prefix, length] of [
    [profile.keyPrefix, profile.keyBytes],
    [profile.compressedKeyPrefix, profile.compressedKeyBytes],
  ] as const) {
    try {
      const point = p256.ProjectivePoint.fromHex(
        parseDerPayload(serialized, new Uint8Array(prefix), length),
      )
      return bytesToBase64(
        concatBytes(new Uint8Array(profile.keyPrefix), point.toRawBytes(false)),
      )
    } catch {}
  }
  return ed25519PublicKeyDer64(parseEd25519PublicKey(serialized))
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
