import * as CBOR from 'cbor-web'
import { SESSION_PROFILE } from '../osBindings/session-profile'

const cbor = CBOR.default ?? CBOR
const major = { bytes: 2, text: 3, array: 4, map: 5, tag: 6, simple: 7 }
const isMapKey = (parent: number | undefined, index: number | undefined) =>
  parent === major.map && index !== undefined && index % 2 === 0
const invalidKind = () => {
  throw new Error('Invalid admission CBOR kind')
}

/** Encodes byte arrays as untagged byte strings and full-width integers without rounding. */
export function encode(value: unknown): Uint8Array<ArrayBuffer> {
  return new Uint8Array(
    cbor.encodeOne(value, {
      canonical: true,
      genTypes: [
        Uint8Array,
        (encoder, bytes) => encoder.pushAny(bytes.slice().buffer),
      ],
    }),
  )
}

/** Decodes one bounded handshake object, retaining its captured-byte boundary. */
export async function decodeFirst(bytes: Uint8Array): Promise<{
  value: unknown
  length: number
}> {
  if (bytes.length > SESSION_PROFILE.transport.maxHandshakeRecordBytes)
    throw new Error('Oversized handshake record')
  return new Promise((resolve, reject) => {
    const decoder = new cbor.Decoder({
      extendedResults: true,
      preferWeb: true,
      preventDuplicateKeys: true,
      max_depth: SESSION_PROFILE.transport.maxHandshakeDepth,
    })
    decoder.on('start', (type, _, parent, index) => {
      if (
        type === major.tag ||
        type === major.array ||
        type === major.bytes ||
        (isMapKey(parent, index) && type !== major.text)
      )
        invalidKind()
    })
    decoder.on('more-bytes', type => {
      if (type === major.simple) invalidKind()
    })
    decoder.on('value', (value, parent, index, additionalInfo) => {
      // BREAK is parser control, not a value or map key.
      if (additionalInfo === 31) return
      if (
        (isMapKey(parent, index) && typeof value !== 'string') ||
        Array.isArray(value) ||
        value === null ||
        value === undefined ||
        typeof value === 'boolean' ||
        typeof value === 'symbol' ||
        cbor.Simple.isSimple(value)
      )
        invalidKind()
    })
    decoder.once('data', ({ value, length }) => {
      resolve({ value, length })
      decoder.destroy()
    })
    decoder.once('error', error => {
      reject(error)
      decoder.destroy()
    })
    decoder.once('end', () => reject(new Error('Empty handshake object')))
    decoder.end(bytes)
  })
}
