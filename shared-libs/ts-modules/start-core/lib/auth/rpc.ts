import { encode } from 'cborg'

/** Reserved root field for JSON-RPC request proofs. */
export const RPC_SIGNATURE_FIELD = 'signature'

/**
 * Length-first deterministic CBOR without the root `signature` field.
 * Floats become eight big-endian IEEE-754 bytes; JSON has no byte-string values.
 * Requires source-aware JSON.parse.
 */
export function rpcSigningBody(request: string): Uint8Array {
  const value: unknown = JSON.parse(
    request,
    (_key: string, value: unknown, context?: { source: string }) => {
      if (typeof value !== 'number') return value
      if (context?.source === undefined)
        throw new Error('RPC signatures require source-aware JSON.parse')
      return parseNumber(context.source)
    },
  )
  if (value === null || typeof value !== 'object' || Array.isArray(value))
    throw new Error('RPC request must be an object')
  validateUnicode(value)
  Reflect.deleteProperty(value, RPC_SIGNATURE_FIELD)
  return encode(value)
}

function parseNumber(value: string): Uint8Array | bigint {
  if (/^-?\d+$/.test(value) && value !== '-0') {
    const integer = BigInt(value)
    if (integer >= -(1n << 63n) && integer < 1n << 64n) return integer
  }
  const number = Number(value)
  if (!Number.isFinite(number)) throw new Error('Non-finite JSON number')
  const bytes = new Uint8Array(8)
  new DataView(bytes.buffer).setFloat64(0, number, false)
  return bytes
}

function validateUnicode(value: unknown): void {
  if (typeof value === 'string') assertUnicode(value)
  else if (Array.isArray(value)) value.forEach(validateUnicode)
  else if (
    value !== null &&
    typeof value === 'object' &&
    !(value instanceof Uint8Array)
  ) {
    for (const [key, item] of Object.entries(value)) {
      assertUnicode(key)
      validateUnicode(item)
    }
  }
}

function assertUnicode(value: string): void {
  if (/[\uD800-\uDFFF]/u.test(value))
    throw new Error('Unpaired surrogate in JSON string')
}
