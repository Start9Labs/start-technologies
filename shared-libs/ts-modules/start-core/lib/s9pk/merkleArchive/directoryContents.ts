import { ArrayBufferReader, Entry } from '.'
import { blake3 } from '@noble/hashes/blake3'
import { serializeVarint } from './varint'
import { FileContents } from './fileContents'
import { compare } from '..'

export class DirectoryContents {
  static readonly headerSize =
    8 + // position: u64 BE
    8 // size: u64 BE
  private constructor(
    readonly contents: { [name: string]: Entry },
    readonly sourceEnd: bigint,
  ) {}
  static async deserialize(
    source: Blob,
    header: ArrayBufferReader,
    sighash: Uint8Array,
    maxSize: bigint,
  ): Promise<DirectoryContents> {
    const position = header.nextU64()
    const size = header.nextU64()
    if (size > maxSize) {
      throw new Error('size is greater than signed')
    }

    let sourceEnd = checkedEnd(source, position, size)
    const tocReader = new ArrayBufferReader(
      await source
        .slice(Number(position), Number(position + size))
        .arrayBuffer(),
    )
    const len = tocReader.nextVarint()
    const entries: { [name: string]: Entry } = Object.create(null)
    for (let i = 0; i < len; i++) {
      const name = tocReader.nextVarstring()
      const entry = await Entry.deserialize(source, tocReader)
      if (entry.contents)
        sourceEnd =
          sourceEnd > entry.contents.sourceEnd
            ? sourceEnd
            : entry.contents.sourceEnd
      entries[name] = entry
    }

    const res = new DirectoryContents(entries, sourceEnd)

    if (!compare(res.sighash(), sighash)) {
      throw new Error('hash sum does not match')
    }

    return res
  }
  sighash(): Uint8Array {
    const hasher = blake3.create({})
    const names = Object.keys(this.contents).sort()
    hasher.update(new Uint8Array(serializeVarint(names.length)))
    for (const name of names) {
      const entry = this.contents[name]
      const nameBuf = new TextEncoder().encode(name)
      hasher.update(new Uint8Array(serializeVarint(nameBuf.length)))
      hasher.update(nameBuf)
      hasher.update(new Uint8Array(entry.hash))
      const sizeBuf = new Uint8Array(8)
      new DataView(sizeBuf.buffer).setBigUint64(0, entry.size)
      hasher.update(sizeBuf)
      hasher.update(new Uint8Array([0]))
    }

    return hasher.digest()
  }
  getPath(path: string[]): Entry | null {
    if (path.length === 0) {
      return null
    }
    const next = this.contents[path[0]]
    const rest = path.slice(1)
    if (next === undefined) {
      return null
    }
    if (rest.length === 0) {
      return next
    }
    if (next.contents instanceof DirectoryContents) {
      return next.contents.getPath(rest)
    }
    return null
  }
}

export function checkedEnd(
  source: Blob,
  position: bigint,
  size: bigint,
): bigint {
  const end = position + size
  if (end > BigInt(source.size) || end > 0xffffffffffffffffn)
    throw new Error('Archive range exceeds source')
  return end
}
