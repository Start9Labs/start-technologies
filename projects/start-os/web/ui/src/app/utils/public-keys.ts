import { auth } from '@start9labs/start-core'

const PUBLIC_KEY_BLOCK =
  /-----BEGIN (PUBLIC KEY|PGP PUBLIC KEY BLOCK)-----[\s\S]*?-----END \1-----/g
const ED25519_SPKI_PREFIX = new Uint8Array([
  48, 42, 48, 5, 6, 3, 43, 101, 112, 3, 33, 0,
])

export function publicKeyPem(raw: Uint8Array): string {
  if (raw.length !== 32) throw new Error('Invalid Ed25519 public key')
  const der = new Uint8Array(ED25519_SPKI_PREFIX.length + raw.length)
  der.set(ED25519_SPKI_PREFIX)
  der.set(raw, ED25519_SPKI_PREFIX.length)
  return `-----BEGIN PUBLIC KEY-----\n${auth.bytesToBase64(der)}\n-----END PUBLIC KEY-----\n`
}

export function parsePemPublicKey(value: string): string {
  const body = auth.normalizeDer64(value)
  if (!/^[A-Za-z0-9+/]+={0,2}$/.test(body))
    throw new Error('Expected a PEM-encoded Ed25519 public key')
  const der = auth.base64ToBytes(body)
  if (
    der.length !== ED25519_SPKI_PREFIX.length + 32 ||
    !ED25519_SPKI_PREFIX.every((byte, i) => der[i] === byte)
  ) {
    throw new Error('Expected Ed25519 public keys')
  }
  return publicKeyPem(der.slice(ED25519_SPKI_PREFIX.length))
}

export function parsePublicKeys(value: string): string[] {
  const blocks = value.match(PUBLIC_KEY_BLOCK)
  if (!blocks?.length || value.replace(PUBLIC_KEY_BLOCK, '').trim()) {
    throw new Error('Expected PEM or armored GPG public keys')
  }
  return [
    ...new Set(
      blocks.map(block =>
        block.startsWith('-----BEGIN PGP PUBLIC KEY BLOCK-----')
          ? block
          : parsePemPublicKey(block),
      ),
    ),
  ]
}
