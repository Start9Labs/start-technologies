import { ed25519 } from '@noble/curves/ed25519'
import { bytesToNumberLE } from '@noble/curves/abstract/utils'
import { sha512 } from '@noble/hashes/sha2'
import { concatBytes } from '@noble/hashes/utils'

/** Matches dalek's strict Ed25519 verification, including its uncofactored equation. */
export function verifyEd25519Strict(
  signature: Uint8Array,
  message: Uint8Array,
  key: Uint8Array,
): boolean {
  return verifyEquation(signature, message, key, new Uint8Array())
}

/** Matches dalek's strict Ed25519ph verification with an RFC8032 context. */
export function verifyEd25519phStrict(
  signature: Uint8Array,
  message: Uint8Array,
  key: Uint8Array,
  context: Uint8Array,
): boolean {
  if (context.length > 255) return false
  const domain = concatBytes(
    new TextEncoder().encode('SigEd25519 no Ed25519 collisions'),
    new Uint8Array([1, context.length]),
    context,
  )
  return verifyEquation(signature, sha512(message), key, domain)
}

function verifyEquation(
  signature: Uint8Array,
  message: Uint8Array,
  key: Uint8Array,
  domain: Uint8Array,
): boolean {
  if (signature.length !== 64 || key.length !== 32) return false
  try {
    const A = ed25519.Point.fromBytes(key, true)
    const R = ed25519.Point.fromBytes(signature.subarray(0, 32), false)
    const s = bytesToNumberLE(signature.subarray(32))
    if (s >= ed25519.CURVE.n || A.isSmallOrder() || R.isSmallOrder())
      return false
    const k =
      bytesToNumberLE(
        sha512(concatBytes(domain, signature.subarray(0, 32), key, message)),
      ) % ed25519.CURVE.n
    return ed25519.Point.BASE.multiplyUnsafe(s)
      .subtract(A.multiplyUnsafe(k))
      .equals(R)
  } catch {
    return false
  }
}
