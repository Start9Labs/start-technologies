import { createHash } from 'node:crypto'
import { createReadStream } from 'node:fs'
import { stat } from 'node:fs/promises'
import { pipeline } from 'node:stream/promises'
import { createGunzip } from 'node:zlib'

async function digest(source) {
  const hash = createHash('sha256')
  for await (const bytes of source) hash.update(bytes)
  return hash.digest('hex')
}

/**
 * Checks provenance of a generated gzip initramfs, not arbitrary archive validity.
 * Prefixes and the uncompressed main CPIO must come from the same private native
 * generation after its executable-closure audit. Public proof files confer no trust.
 * The caller excludes writes until verification and consumption finish.
 */
export async function verifyGeneratedArchive(archive, prefixes, mainCpio) {
  let offset = 0
  for (const prefix of prefixes) {
    const { size } = await stat(prefix)
    if (size === 0) continue
    const expected = await digest(createReadStream(prefix))
    const actual = await digest(
      createReadStream(archive, { start: offset, end: offset + size - 1 }),
    )
    if (actual !== expected) throw Error('Generated initramfs prefix differs')
    offset += size
  }
  const expected = await digest(createReadStream(mainCpio))
  const { size } = await stat(archive)
  const gunzip = createGunzip()
  const actual = await pipeline(
    createReadStream(archive, { start: offset }),
    gunzip,
    digest,
  )
  // zlib stops at zero padding without inspecting subsequent bytes.
  if (gunzip.bytesWritten !== size - offset)
    throw Error('Generated initramfs contains unconsumed compressed input')
  if (actual !== expected)
    throw Error('Generated initramfs main CPIO stream differs')
}
