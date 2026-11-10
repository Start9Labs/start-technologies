declare module 'cbor-web' {
  interface Encoder {
    pushAny(value: unknown): boolean
  }
  interface First {
    value: unknown
    length: number
  }
  export function encodeOne(
    value: unknown,
    options: {
      canonical: boolean
      genTypes: [
        Uint8ArrayConstructor,
        (encoder: Encoder, value: Uint8Array) => boolean,
      ]
    },
  ): Uint8Array
  export function decodeFirstSync(
    value: Uint8Array,
    options: {
      extendedResults: true
      preferWeb: true
      preventDuplicateKeys: true
      max_depth: number
    },
  ): First
  const cbor: {
    encodeOne: typeof encodeOne
    decodeFirstSync: typeof decodeFirstSync
  }
  export default cbor
}
