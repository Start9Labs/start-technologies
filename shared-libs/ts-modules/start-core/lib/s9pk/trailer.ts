import { ed25519PublicKeyDer64, parseEd25519PublicKey } from '../auth/signature'
import { verifyMerkleCommitment } from '../auth/commitment'
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
    const offset = Number(start) + record.offset
    let value: Record<string, unknown>
    try {
      const parsed: unknown = strictJson(
        new TextDecoder('utf-8', { fatal: true }).decode(
          raw.slice(record.offset + 10, record.end),
        ),
      )
      if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed))
        throw new Error('Invalid signature record')
      value = parsed as Record<string, unknown>
      if (
        typeof value.signer !== 'string' ||
        typeof value.signature !== 'string'
      )
        throw new Error('Invalid signature record')
    } catch {
      diagnose(
        'invalid-record',
        'Ignoring malformed supplemental signature record',
        offset,
      )
      continue
    }
    try {
      const signer = value.signer as string
      if (
        !verifyMerkleCommitment(
          signer,
          value.signature as string,
          archive.root,
          archive.signer.maxSize,
          's9pk',
        )
      ) {
        throw new Error('Invalid signature')
      }
      signers.add(ed25519PublicKeyDer64(parseEd25519PublicKey(signer)))
    } catch {
      diagnose(
        'invalid-signature',
        'Ignoring invalid or unsupported supplemental signature',
        offset,
      )
    }
  }
  return result()
}

function strictJson(text: string): unknown {
  let pos = 0
  const fail = (): never => {
    throw new Error('Malformed JSON or duplicate key')
  }
  const whitespace = () => {
    while (/[\x20\t\r\n]/.test(text[pos] ?? '') && pos < text.length) pos++
  }
  const string = (): string => {
    const pattern =
      /"(?:[^"\\\u0000-\u001f]|\\(?:["\\/bfnrt]|u[0-9a-fA-F]{4}))*"/y
    pattern.lastIndex = pos
    const match = pattern.exec(text)
    if (!match) return fail()
    pos = pattern.lastIndex
    const value = JSON.parse(match[0]) as string
    if (
      /[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/u.test(
        value,
      )
    )
      fail()
    return value
  }
  const value = (depth: number): unknown => {
    if (depth > 64) return fail()
    whitespace()
    if (text[pos] === '"') return string()
    if (text[pos] === '{') {
      pos++
      whitespace()
      const result = Object.create(null) as Record<string, unknown>
      const keys = new Set<string>()
      if (text[pos] === '}') {
        pos++
        return result
      }
      while (true) {
        const key = string()
        if (keys.has(key)) return fail()
        keys.add(key)
        whitespace()
        if (text[pos++] !== ':') return fail()
        result[key] = value(depth + 1)
        whitespace()
        const delimiter = text[pos++]
        if (delimiter === '}') return result
        if (delimiter !== ',') return fail()
        whitespace()
      }
    }
    if (text[pos] === '[') {
      pos++
      whitespace()
      const result: unknown[] = []
      if (text[pos] === ']') {
        pos++
        return result
      }
      while (true) {
        result.push(value(depth + 1))
        whitespace()
        const delimiter = text[pos++]
        if (delimiter === ']') return result
        if (delimiter !== ',') return fail()
      }
    }
    for (const literal of ['true', 'false', 'null']) {
      if (text.startsWith(literal, pos)) {
        pos += literal.length
        return JSON.parse(literal)
      }
    }
    const number = /-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?/y
    number.lastIndex = pos
    const match = number.exec(text)
    if (!match || !Number.isFinite(Number(match[0]))) return fail()
    pos = number.lastIndex
    return Number(match[0])
  }
  const result = value(0)
  whitespace()
  if (pos !== text.length) fail()
  return result
}
