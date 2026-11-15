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
  export class Decoder {
    constructor(options: {
      extendedResults: true
      preferWeb: true
      preventDuplicateKeys: true
      max_depth: number
    })
    on(
      event: 'start' | 'start-string' | 'more-bytes',
      listener: (
        type: number,
        length: number | symbol,
        parent: number | undefined,
        index: number | undefined,
      ) => void,
    ): this
    on(
      event: 'value',
      listener: (
        value: unknown,
        parent: number | undefined,
        index: number | undefined,
        additionalInfo: number,
      ) => void,
    ): this
    once(event: 'data', listener: (value: First) => void): this
    once(event: 'error', listener: (error: Error) => void): this
    once(event: 'end', listener: () => void): this
    end(bytes: Uint8Array): void
    destroy(): void
  }
  export const Simple: { isSimple(value: unknown): boolean }
  const cbor: {
    encodeOne: typeof encodeOne
    Decoder: typeof Decoder
    Simple: typeof Simple
  }
  export default cbor
}
