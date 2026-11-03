import { ed25519ph } from '@noble/curves/ed25519'
import { parseDerPayload, PH_SIGNATURE_PREFIX } from '../auth/signature'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { S9pk } from '../s9pk'
import { ed25519PublicKeyDer64, parseEd25519PublicKey } from '../auth/signature'
import {
  merkleCommitmentBytes,
  verifyMerkleCommitment,
} from '../auth/commitment'

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
  largeBound: { bound: string; signature: string }
  invalidPoint: { signer: string; signature: string }
  embedded: string
  messageHex: string
  taggedMessageHex: string
  endorsements: { signer: string; signature: string }[]
}
const base = bytes('base.s9pk')
const root = Uint8Array.from(
  Buffer.from(vectors.commitment.rootSighash, 'base64'),
)
function tlv(type: number, value: Uint8Array): Uint8Array<ArrayBuffer> {
  const result = new Uint8Array(10 + value.length)
  const view = new DataView(result.buffer)
  view.setUint16(0, type)
  view.setBigUint64(2, BigInt(value.length))
  result.set(value, 10)
  return result
}
const json = (value: unknown) => new TextEncoder().encode(JSON.stringify(value))
const good = () => tlv(1, json(vectors.endorsements[0]))
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

test('native-generated ph, pure and legacy signatures use the exact frozen commitment', () => {
  expect(vectors.commitment).toEqual({
    type: 'merkle-archive',
    rootSighash: 'CEhWFKyWwTpG30snr5ZcatODIjlEDUuTrfA0uV0ZJ1g',
    rootMaxsize: 380,
  })
  expect(vectors.embedded).toBe(
    'MCowBQYDK2VwAyEA6kpsY+KcUgq+9VB7Ey7F+ZVHdq6+vnuSQh7qaRRG0iw=',
  )
  expect(Buffer.from(merkleCommitmentBytes(root, 380n)).toString('hex')).toBe(
    vectors.messageHex,
  )
  expect(vectors.messageHex).toBe(
    '08485614ac96c13a46df4b27af965c6ad3832239440d4b93adf034b95d192758000000000000017c',
  )
  expect(vectors.taggedMessageHex).toBe(
    '53746172742d436f6d6d69746d656e742d53696720763100047339706b0208485614ac96c13a46df4b27af965c6ad3832239440d4b93adf034b95d192758000000000000017c',
  )
  for (const record of vectors.endorsements) {
    expect(
      verifyMerkleCommitment(
        record.signer,
        record.signature,
        root,
        380n,
        's9pk',
      ),
    ).toBe(true)
    expect(
      verifyMerkleCommitment(
        record.signer,
        record.signature,
        root,
        381n,
        's9pk',
      ),
    ).toBe(false)
    expect(
      verifyMerkleCommitment(
        record.signer,
        record.signature,
        root,
        380n,
        'other',
      ),
    ).toBe(false)
    expect(
      verifyMerkleCommitment(
        vectors.embedded,
        record.signature,
        root,
        380n,
        's9pk',
      ),
    ).toBe(false)
    expect(
      verifyMerkleCommitment(
        record.signer,
        record.signature,
        new Uint8Array(32),
        380n,
        's9pk',
      ),
    ).toBe(false)
  }
  expect(
    verifyMerkleCommitment(
      vectors.endorsements[0].signer,
      vectors.largeBound.signature,
      root,
      BigInt(vectors.largeBound.bound),
      's9pk',
    ),
  ).toBe(true)
  expect(
    verifyMerkleCommitment(
      vectors.endorsements[0].signer,
      vectors.largeBound.signature,
      root,
      9007199254740992n,
      's9pk',
    ),
  ).toBe(false)
  expect(
    verifyMerkleCommitment(
      vectors.invalidPoint.signer,
      vectors.invalidPoint.signature,
      root,
      380n,
      's9pk',
    ),
  ).toBe(false)
  const large = 0xffffffffffffffffn
  expect(
    Buffer.from(merkleCommitmentBytes(root, large))
      .subarray(32)
      .toString('hex'),
  ).toBe('ffffffffffffffff')
  expect(() => merkleCommitmentBytes(root, large + 1n)).toThrow()
})

test('the browser sees only verified canonical identities and immutable diagnostics', async () => {
  const packageFile = new Blob([bytes('mixed.s9pk')])
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
  expect(packageInfo.diagnostics.map(item => item.kind)).toEqual([
    'invalid-signature',
  ])
  expect(Object.isFrozen(packageInfo.signers)).toBe(true)
  expect(Object.isFrozen(packageInfo.diagnostics)).toBe(true)
  for (const record of vectors.endorsements) {
    expect(ed25519PublicKeyDer64(parseEd25519PublicKey(record.signer))).toBe(
      record.signer,
    )
    expect(
      ed25519PublicKeyDer64(
        parseEd25519PublicKey(
          `-----BEGIN PUBLIC KEY-----\n${record.signer}\n-----END PUBLIC KEY-----\n`,
        ),
      ),
    ).toBe(record.signer)
    expect(
      ed25519PublicKeyDer64(
        parseEd25519PublicKey(
          record.signer
            .replace(/\+/g, '-')
            .replace(/\//g, '_')
            .replace(/=/g, ''),
        ),
      ),
    ).toBe(record.signer)
  }
  expect(() =>
    parseEd25519PublicKey(Buffer.alloc(32).toString('base64')),
  ).toThrow()
  expect(() => parseEd25519PublicKey(vectors.embedded + 'AA==')).toThrow()
})

test.each([
  '{"signer":"x","sign\\u0065r":"y","signature":"z"}',
  '{"signer":"x","signature":"y","u":{"a":1,"\\u0061":2}}',
  '{"signer":1,"signature":"x"}',
  '{}',
  '[]',
  'null',
  '['.repeat(70) + '0' + ']'.repeat(70),
  '{"signer":"\\uD800","signature":"x"}',
])('malformed known records do not hide valid siblings: %s', async text => {
  const result = await read(
    file([tlv(1, new TextEncoder().encode(text)), good()]),
  )
  expect(result.signers).toEqual([
    vectors.embedded,
    vectors.endorsements[0].signer,
  ])
  expect(result.diagnostics.map(item => item.kind)).toEqual(['invalid-record'])
})

test('native-rejected key encodings never become verified browser signers', async () => {
  const signer = vectors.endorsements[0].signer
  for (const invalid of [
    signer.replace('k8o=', 'k8p='),
    signer + ' ',
    `-----BEGIN PUBLIC KEY-----\n${signer}\n-----END OTHER KEY-----\n`,
  ]) {
    expect(() => parseEd25519PublicKey(invalid)).toThrow()
    const result = await read(
      file([tlv(1, json({ ...vectors.endorsements[0], signer: invalid }))]),
    )
    expect(result.signers).toEqual([vectors.embedded])
    expect(result.diagnostics[0].kind).toBe('invalid-signature')
  }
})

test('unknown records and fields are silent; unsupported algorithms and invalid UTF8 warn', async () => {
  const extra = { ...vectors.endorsements[0], unknown: { nested: ['abc', 1] } }
  expect(
    (await read(file([tlv(65535, new Uint8Array()), tlv(1, json(extra))])))
      .diagnostics,
  ).toEqual([])
  const unsupported = { ...vectors.endorsements[0], signature: 'MAA=' }
  const result = await read(
    file([tlv(1, json(unsupported)), tlv(1, new Uint8Array([255])), good()]),
  )
  expect(result.signers.length).toBe(2)
  expect(result.diagnostics.map(item => item.kind)).toEqual([
    'invalid-signature',
    'invalid-record',
  ])
})

test('malformed framing discards all endorsements, and checks unsigned lengths before conversion', async () => {
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

test('overlap in lazy files and TOCs is optional metadata damage, never a truncation boundary', async () => {
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

test('embedded verification stays mandatory; version and complete header parsing share one owner', async () => {
  const unsupported = new Uint8Array(await file([good()]).arrayBuffer())
  unsupported[unsupported.length - 1] = 50
  expect((await read(new Blob([unsupported]))).diagnostics[0].kind).toBe(
    'unsupported-version',
  )
  const bad = base.slice()
  bad[35] ^= 1
  await expect(read(new Blob([bad]))).rejects.toThrow(
    'signature verification failed',
  )
  for (const header of [[], [59], [59, 59], [0, 0, 2]]) {
    await expect(
      S9pk.getVersion(new Blob([new Uint8Array(header)])),
    ).rejects.toThrow()
  }
  expect(await S9pk.getVersion(new Blob([new Uint8Array([59, 59, 1])]))).toBe(1)
  await expect(read(new Blob([new Uint8Array([59, 59, 1])]))).rejects.toThrow()
})

test('lazy files from unused architectures still reject endpoint overflow and EOF overrun', async () => {
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

test('strict verification rejects vectors noble accepts after clearing the cofactor', () => {
  const fixture = JSON.parse(
    readFileSync(join(directory, 'vectors.json'), 'utf8'),
  )
  for (const signature of [fixture.smallR, fixture.torsionR]) {
    expect(
      ed25519ph.verify(
        parseDerPayload(signature, PH_SIGNATURE_PREFIX, 64),
        merkleCommitmentBytes(root, 380n),
        parseEd25519PublicKey(vectors.endorsements[0].signer),
        { context: new TextEncoder().encode('s9pk'), zip215: false },
      ),
    ).toBe(true)
    expect(
      verifyMerkleCommitment(
        vectors.endorsements[0].signer,
        signature,
        root,
        380n,
        's9pk',
      ),
    ).toBe(false)
  }
})

test('an external commitment pins the decoded root bytes without weakening embedded verification', async () => {
  const commitment = { type: 'merkle-archive' as const, ...vectors.commitment }
  const file = new Blob([base])
  expect((await S9pk.deserialize(file, commitment)).signers).toEqual([
    vectors.embedded,
  ])
  await expect(
    S9pk.deserialize(file, {
      ...commitment,
      rootSighash: Buffer.alloc(32).toString('base64'),
    }),
  ).rejects.toThrow('merkle root mismatch')
})

test('optional footer I/O failure reports a diagnostic without invalidating verified base metadata', async () => {
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
