import { mkdir, mkdtemp, rm } from 'node:fs/promises'
import { join } from 'node:path'

import { verifyGeneratedArchive } from './fde-initramfs-archive.mjs'
import { capturedStreams } from './fde-initramfs-producer.mjs'

/**
 * The producer admits its sources/closure and joins all writers before returning.
 * The consumer finishes archive use before returning; neither callback retains writers.
 */
export async function withPrivateInitramfs(parent, produce, consume) {
  const directory = await mkdtemp(join(parent, '.fde-initramfs-'))
  try {
    const archive = join(directory, 'initrd')
    const captures = join(directory, 'captures')
    await mkdir(captures, { mode: 0o700 })
    await produce({ directory, archive, captures })
    const streams = capturedStreams(captures)
    await verifyGeneratedArchive(archive, streams.prefixes, streams.mainCpio)
    return await consume(archive)
  } finally {
    await rm(directory, { recursive: true })
  }
}
