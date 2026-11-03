import { verifyEd25519Strict, verifyEd25519phStrict } from './ed25519'
import {
  base64ToBytes,
  normalizeDer64,
  parseDerPayload,
  parseEd25519PublicKey,
  PH_SIGNATURE_PREFIX,
  SIGNATURE_PREFIX,
  signingContext,
  signingMessage,
} from './signature'

/** The signed Merkle root and its exact unsigned 64-bit bound. */
export function merkleCommitmentBytes(
  root: Uint8Array,
  bound: bigint,
): Uint8Array {
  if (root.length !== 32 || bound < 0n || bound > 0xffffffffffffffffn) {
    throw new Error('Invalid Merkle commitment')
  }
  const bytes = new Uint8Array(40)
  bytes.set(root)
  new DataView(bytes.buffer).setBigUint64(32, bound, false)
  return bytes
}

/** Verifies canonical Ed25519ph, pure Ed25519, and legacy mislabeled ph commitments. */
export function verifyMerkleCommitment(
  signer: string,
  signature: string,
  root: Uint8Array,
  bound: bigint,
  context: string,
): boolean {
  const publicKey = parseEd25519PublicKey(signer)
  const message = merkleCommitmentBytes(root, bound)
  const contextBytes = signingContext(context)
  const der = base64ToBytes(normalizeDer64(signature))
  const ph = der.length === PH_SIGNATURE_PREFIX.length + 64
  const raw = parseDerPayload(
    signature,
    ph ? PH_SIGNATURE_PREFIX : SIGNATURE_PREFIX,
    64,
  )
  if (!ph) {
    const tagged = signingMessage(
      new TextEncoder().encode('Start-Commitment-Sig v1\0'),
      context,
      2,
      message,
    )
    if (verifyEd25519Strict(raw, tagged, publicKey)) return true
  }
  return verifyEd25519phStrict(raw, message, publicKey, contextBytes)
}
