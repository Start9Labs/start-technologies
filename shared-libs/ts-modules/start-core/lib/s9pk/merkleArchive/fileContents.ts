import { blake3 } from '@noble/hashes/blake3'
import { ArrayBufferReader } from '.'
import { compare } from '..'
import { checkedEnd } from './directoryContents'

export class FileContents {
  private constructor(
    readonly contents: Blob,
    readonly sourceEnd: bigint,
  ) {}
  static deserialize(
    source: Blob,
    header: ArrayBufferReader,
    size: bigint,
  ): FileContents {
    const position = header.nextU64()
    const end = checkedEnd(source, position, size)
    return new FileContents(source.slice(Number(position), Number(end)), end)
  }
  async verified(hash: Uint8Array): Promise<ArrayBuffer> {
    const res = await this.contents.arrayBuffer()
    if (!compare(hash, blake3(new Uint8Array(res)))) {
      throw new Error('hash sum mismatch')
    }
    return res
  }
}
