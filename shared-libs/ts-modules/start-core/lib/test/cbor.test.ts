import { decodeFirst, encode } from '../auth/cbor'

const read = (bytes: number[]) => decodeFirst(new Uint8Array(bytes))

test.each([
  ['typed byte-array tag', [0xd8, 0x40, 0x41, 1]],
  ['bignum tag', [0xc2, 0x41, 1]],
  ['unknown semantic tag', [0xd9, 3, 0xe7, 1]],
  ['integer map key', [0xa1, 0, 1]],
  ['byte-string map key', [0xa1, 0x41, 0x61, 1]],
  ['map as a map key', [0xa1, 0xa1, 0x61, 0x61, 1, 1]],
  ['empty array', [0x80]],
  ['nested array', [0xa1, 0x61, 0x61, 0x81, 1]],
  ['indefinite bytes', [0xa1, 0x61, 0x61, 0x5f, 0x41, 1, 0xff]],
  ['integral float', [0xf9, 0x3c, 0]],
  ['boolean', [0xf4]],
  ['null', [0xf6]],
  ['undefined', [0xf7]],
  ['simple value', [0xf8, 24]],
] as const)(
  'admission rejects %s before schema interpretation',
  async (_, bytes) => {
    await expect(
      Promise.resolve().then(() => read([...bytes])),
    ).rejects.toThrow()
  },
)

test('first-object length preserves field order and leaves trailing data to its caller', async () => {
  const bytes = [0xbf, 0x61, 0x7a, 1, 0x61, 0x61, 2, 0xff, 0xf6]
  const result = await read(bytes)
  expect(result).toEqual({ value: { z: 1, a: 2 }, length: 8 })
  expect(bytes.slice(result.length)).toEqual([0xf6])
  expect(await read([0xbf, 0xff])).toEqual({ value: {}, length: 2 })
  expect(await read([0x7f, 0x61, 0x61, 0xff])).toEqual({
    value: 'a',
    length: 4,
  })
})

test('full-width integers and definite bytes remain lossless', async () => {
  const value = {
    minimum: -(1n << 63n),
    maximum: (1n << 64n) - 1n,
    binary: new Uint8Array([0, 0xff]),
  }
  expect((await decodeFirst(encode(value))).value).toEqual(value)
})

test.each([
  ['empty input', []],
  ['truncated map', [0xa1, 0x61, 0x61]],
  ['truncated bytes', [0x42, 1]],
  ['invalid break', [0xff]],
  ['duplicate key', [0xa2, 0x61, 0x61, 1, 0x61, 0x61, 2]],
  [
    'depth limit',
    [...Array.from({ length: 18 }, () => [0xa1, 0x61, 0x61]).flat(), 0],
  ],
  ['record size limit', new Array(1025).fill(0)],
] as const)(
  'admission rejects %s without a pending decoder',
  async (_, bytes) => {
    await expect(
      Promise.resolve().then(() => read([...bytes])),
    ).rejects.toThrow()
  },
)
