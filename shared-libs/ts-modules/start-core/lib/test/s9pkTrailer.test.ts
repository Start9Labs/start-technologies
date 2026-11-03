import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { S9pk } from '../s9pk'
import { ed25519PublicKeyDer64, parseEd25519PublicKey } from '../auth/signature'

const directory = join(
  __dirname,
  '../../../../crates/start-core/testdata/s9pk-trailer',
)
const bytes = (name: string) =>
  Uint8Array.from(readFileSync(join(directory, name)))
const vectors = JSON.parse(
  readFileSync(join(directory, 'vectors.json'), 'utf8'),
) as {
  commitment: { rootSighash: string; rootMaxsize: number }
  embedded: string
  endorsements: { signer: string; signature: string }[]
}
const base = bytes('base.s9pk')
function tlv(type: number, value: Uint8Array): Uint8Array<ArrayBuffer> {
  const result = new Uint8Array(10 + value.length)
  const view = new DataView(result.buffer)
  view.setUint16(0, type)
  view.setBigUint64(2, BigInt(value.length))
  result.set(value, 10)
  return result
}
function endorsement(
  signer: string,
  signature: Uint8Array,
): Uint8Array<ArrayBuffer> {
  const key = Buffer.from(signer, 'base64')
  const result = new Uint8Array(8 + key.length + signature.length)
  new DataView(result.buffer).setBigUint64(0, BigInt(key.length))
  result.set(key, 8)
  result.set(signature, 8 + key.length)
  return result
}
const good = () =>
  tlv(
    1,
    endorsement(
      vectors.endorsements[0].signer,
      Buffer.from(vectors.endorsements[0].signature, 'base64'),
    ),
  )
function file(region: Uint8Array[], archive = base): Blob {
  const size = region.reduce((sum, item) => sum + item.length, 0)
  const footer = new Uint8Array(16)
  new DataView(footer.buffer).setBigUint64(0, BigInt(size))
  footer.set(new TextEncoder().encode('S9PKEXT1'), 8)
  return new Blob([
    Uint8Array.from(archive),
    ...region.map(item => Uint8Array.from(item)),
    footer,
  ])
}
const read = (blob: Blob) => S9pk.deserialize(blob, null)

test('native binary endorsements expose claimed keys and immutable parse diagnostics', async () => {
  const packageFile = new Blob([bytes('endorsed.s9pk')])
  expect(await S9pk.getVersion(packageFile)).toBe(2)
  const packageInfo = await read(packageFile)
  expect(packageInfo.manifest.id).toBe('trust-fixture')
  expect(packageInfo.manifest.dependencies).toEqual({})
  expect(await packageInfo.icon()).toMatch(/^data:image\/svg\+xml;base64,/)
  expect(await packageInfo.license()).toBe('MIT')
  expect(packageInfo.signers).toEqual([
    vectors.embedded,
    ...vectors.endorsements.map(record => record.signer),
  ])
  expect(packageInfo.diagnostics).toEqual([])
  expect(Object.isFrozen(packageInfo.signers)).toBe(true)
  expect(Object.isFrozen(packageInfo.diagnostics)).toBe(true)
})

test('browser parsing does not authenticate embedded or supplemental signatures', async () => {
  const bad = base.slice()
  bad[35] ^= 1
  const signature = Buffer.from(vectors.endorsements[0].signature, 'base64')
  signature[signature.length - 1] ^= 1
  const result = await read(
    file([tlv(1, endorsement(vectors.endorsements[0].signer, signature))], bad),
  )
  expect(result.signers).toEqual([
    vectors.embedded,
    vectors.endorsements[0].signer,
  ])
  expect(result.diagnostics).toEqual([])
})

test('duplicate claimed identities appear once and unknown TLV types are ignored', async () => {
  const result = await read(
    file([
      good(),
      tlv(65535, new Uint8Array([255, 0, 5])),
      good(),
      tlv(1, endorsement(vectors.embedded, new Uint8Array([0]))),
    ]),
  )
  expect(result.signers).toEqual([
    vectors.embedded,
    vectors.endorsements[0].signer,
  ])
  expect(result.diagnostics).toEqual([])
})

test.each([
  { value: [] },
  { value: [0] },
  { value: [0, 0, 0, 0, 0, 0, 0, 0] },
  { value: Array(8).fill(255) },
])(
  'malformed binary record lengths do not hide valid siblings: $value',
  async ({ value }) => {
    const result = await read(file([tlv(1, new Uint8Array(value)), good()]))
    expect(result.signers).toEqual([
      vectors.embedded,
      vectors.endorsements[0].signer,
    ])
    expect(result.diagnostics.map(item => item.kind)).toEqual([
      'invalid-record',
    ])
  },
)

test('a record requires a supported public-key DER and nonempty signature bytes', async () => {
  const validKey = vectors.endorsements[0].signer
  for (const value of [
    endorsement(validKey, new Uint8Array()),
    endorsement(Buffer.alloc(32).toString('base64'), new Uint8Array([0])),
    endorsement(
      Buffer.concat([
        Buffer.from(validKey, 'base64'),
        Buffer.from([0]),
      ]).toString('base64'),
      new Uint8Array([0]),
    ),
  ]) {
    const result = await read(file([tlv(1, value), good()]))
    expect(result.signers).toEqual([vectors.embedded, validKey])
    expect(result.diagnostics[0].kind).toBe('invalid-record')
  }
  expect(ed25519PublicKeyDer64(parseEd25519PublicKey(validKey))).toBe(validKey)
})

test('malformed TLV framing discards every supplemental candidate before processing records', async () => {
  const huge = new Uint8Array(10)
  new DataView(huge.buffer).setBigUint64(2, 0xffffffffffffffffn)
  for (const bad of [new Uint8Array([0]), huge]) {
    const result = await read(file([good(), bad]))
    expect(result.signers).toEqual([vectors.embedded])
    expect(result.diagnostics[0].kind).toBe('invalid-framing')
  }
  for (const size of [1048576 - 10, 1048576 - 9]) {
    const result = await read(file([tlv(22, new Uint8Array(size))]))
    expect(result.diagnostics.length).toBe(size === 1048576 - 10 ? 0 : 1)
  }
  const underflow = new Uint8Array(await file([]).arrayBuffer())
  new DataView(underflow.buffer).setBigUint64(
    underflow.length - 16,
    0xffffffffffffffffn,
  )
  expect((await read(new Blob([underflow]))).diagnostics[0].kind).toBe(
    'invalid-framing',
  )
})

test('overlapping lazy files and TOCs cannot become trailer boundaries', async () => {
  const overlap = await read(new Blob([bytes('overlap.s9pk')]))
  expect(overlap.signers).toEqual([vectors.embedded])
  expect(overlap.diagnostics[0].kind).toBe('invalid-framing')
  const header = new DataView(base.buffer)
  const toc = base.slice(
    Number(header.getBigUint64(139)),
    Number(header.getBigUint64(139) + header.getBigUint64(147)),
  )
  const moved = base.slice()
  new DataView(moved.buffer).setBigUint64(139, BigInt(base.length + 10))
  const result = await read(file([tlv(22, toc), good()], moved))
  expect(result.signers).toEqual([vectors.embedded])
  expect(result.diagnostics[0].kind).toBe('invalid-framing')
})

test('unsupported extensions warn while version and complete header parsing share one owner', async () => {
  const unsupported = new Uint8Array(await file([good()]).arrayBuffer())
  unsupported[unsupported.length - 1] = 50
  expect((await read(new Blob([unsupported]))).diagnostics[0].kind).toBe(
    'unsupported-version',
  )
  for (const header of [[], [59], [59, 59], [0, 0, 2]]) {
    await expect(
      S9pk.getVersion(new Blob([new Uint8Array(header)])),
    ).rejects.toThrow()
  }
  expect(await S9pk.getVersion(new Blob([new Uint8Array([59, 59, 1])]))).toBe(1)
  await expect(read(new Blob([new Uint8Array([59, 59, 1])]))).rejects.toThrow()
})

test('unused architectures still reject endpoint overflow and EOF overrun', async () => {
  const name = Buffer.from('dormant.squashfs')
  const offset = Buffer.from(base).indexOf(name) + name.length + 32 + 8 + 1
  for (const position of [0xffffffffffffffffn, BigInt(base.length)]) {
    const bad = base.slice()
    new DataView(bad.buffer).setBigUint64(offset, position)
    await expect(read(new Blob([bad]))).rejects.toThrow(
      'Archive range exceeds source',
    )
  }
})

test('external commitments still pin decoded Merkle roots without authenticating signers', async () => {
  const commitment = { type: 'merkle-archive' as const, ...vectors.commitment }
  const archive = new Blob([base])
  expect((await S9pk.deserialize(archive, commitment)).signers).toEqual([
    vectors.embedded,
  ])
  await expect(
    S9pk.deserialize(archive, {
      ...commitment,
      rootSighash: Buffer.alloc(32).toString('base64'),
    }),
  ).rejects.toThrow('merkle root mismatch')
})

test('optional footer I/O failure leaves base metadata available with a diagnostic', async () => {
  class UnreadableTail extends Blob {
    override slice(start?: number, end?: number, type?: string): Blob {
      const part = super.slice(start, end, type)
      if (start === -16)
        Object.defineProperty(part, 'arrayBuffer', {
          value: async () => {
            throw new Error('fixture read failure')
          },
        })
      return part
    }
  }
  const packageInfo = await read(new UnreadableTail([base]))
  expect(packageInfo.signers).toEqual([vectors.embedded])
  expect(packageInfo.diagnostics[0].kind).toBe('read-failed')
})
