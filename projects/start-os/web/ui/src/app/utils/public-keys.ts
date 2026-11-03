import { auth } from '@start9labs/start-core'

const PUBLIC_KEY_BLOCK =
  /-----BEGIN (PUBLIC KEY|PGP PUBLIC KEY BLOCK)-----[\s\S]*?-----END \1-----/g
export function publicKeyPem(raw: Uint8Array): string {
  return `-----BEGIN PUBLIC KEY-----\n${auth.ed25519PublicKeyDer64(raw)}\n-----END PUBLIC KEY-----\n`
}

export function parsePemPublicKey(value: string): string {
  return publicKeyPem(auth.parseEd25519PublicKey(auth.normalizeDer64(value)))
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
