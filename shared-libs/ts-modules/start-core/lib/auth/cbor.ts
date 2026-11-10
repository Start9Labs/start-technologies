import * as CBOR from 'cbor-web'
import { SESSION_PROFILE } from '../osBindings/session-profile'

const cbor = CBOR.default ?? CBOR

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
export function decodeFirst(bytes: Uint8Array): {
  value: unknown
  length: number
} {
  if (bytes.length > SESSION_PROFILE.transport.maxHandshakeRecordBytes)
    throw new Error('Oversized handshake record')
  const { value, length } = cbor.decodeFirstSync(bytes, {
    extendedResults: true,
    preferWeb: true,
    preventDuplicateKeys: true,
    max_depth: SESSION_PROFILE.transport.maxHandshakeDepth,
  })
  return { value, length }
}
