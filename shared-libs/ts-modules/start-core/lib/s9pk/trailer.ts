import {
  bytesToBase64,
  ed25519PublicKeyDer64,
  parseEd25519PublicKey,
} from '../auth/signature'
import { MerkleArchive } from './merkleArchive'
import { s9pkHeaderSize } from '.'

export interface S9pkDiagnostic {
  readonly kind: string
  readonly message: string
  readonly offset?: number
}

export async function readTrailer(
  source: Blob,
  archive: MerkleArchive,
): Promise<{
  signers: readonly string[]
  diagnostics: readonly S9pkDiagnostic[]
}> {
  const signers = new Set([ed25519PublicKeyDer64(archive.signer.pubkey)])
  const diagnostics: S9pkDiagnostic[] = []
  const diagnose = (kind: string, message: string, offset: number) => {
    diagnostics.push(Object.freeze({ kind, message, offset }))
  }
  const result = () => ({
    signers: Object.freeze([...signers]),
    diagnostics: Object.freeze(diagnostics),
  })
  const read = async (
    slice: Blob,
    offset: number,
  ): Promise<Uint8Array<ArrayBuffer> | null> => {
    try {
      return new Uint8Array(await slice.arrayBuffer())
    } catch {
      diagnose(
        'read-failed',
        'Unable to read optional supplemental metadata',
        offset,
      )
      return null
    }
  }
  if (source.size < 16) return result()
  const footer = await read(source.slice(-16), source.size - 16)
  if (!footer) return result()
  if (new TextDecoder().decode(footer.slice(8, 15)) !== 'S9PKEXT')
    return result()
  if (footer[15] !== 49) {
    diagnose(
      'unsupported-version',
      'Ignoring unsupported supplemental trailer version',
      source.size - 8,
    )
    return result()
  }
  const length = new DataView(footer.buffer).getBigUint64(0)
  const start = BigInt(source.size) - 16n - length
  if (
    length > 1048576n ||
    start < BigInt(s9pkHeaderSize) ||
    start < archive.contents.sourceEnd
  ) {
    diagnose(
      'invalid-framing',
      'Ignoring malformed or overlapping supplemental trailer',
      source.size - 16,
    )
    return result()
  }
  const raw = await read(
    source.slice(Number(start), source.size - 16),
    Number(start),
  )
  if (!raw) return result()
  const view = new DataView(raw.buffer)
  const records: { type: number; offset: number; end: number }[] = []
  let pos = 0
  while (pos < raw.length) {
    if (raw.length - pos < 10) {
      diagnose(
        'invalid-framing',
        'Ignoring malformed supplemental trailer',
        Number(start) + pos,
      )
      return result()
    }
    const size = view.getBigUint64(pos + 2)
    if (size > BigInt(raw.length - pos - 10)) {
      diagnose(
        'invalid-framing',
        'Ignoring malformed supplemental trailer',
        Number(start) + pos,
      )
      return result()
    }
    const end = pos + 10 + Number(size)
    records.push({ type: view.getUint16(pos), offset: pos, end })
    pos = end
  }
  for (const record of records) {
    if (record.type !== 1) continue
    try {
      const value = record.offset + 10
      if (record.end - value < 8) throw new Error('Missing public key length')
      const keyLength = view.getBigUint64(value)
      if (keyLength === 0n || keyLength >= BigInt(record.end - value - 8))
        throw new Error('Invalid public key length')
      const keyEnd = value + 8 + Number(keyLength)
      const signer = bytesToBase64(raw.subarray(value + 8, keyEnd))
      parseEd25519PublicKey(signer)
      signers.add(signer)
    } catch {
      diagnose(
        'invalid-record',
        'Ignoring malformed supplemental signature record',
        Number(start) + record.offset,
      )
    }
  }
  return result()
}
