import { lstat, mkdir, mkdtemp, readFile, realpath, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import {
  checkedDirectory,
  copyRootAliases,
  filesUnder,
} from './fde-initramfs-paths.mjs'
import { requireNativeProducer } from './fde-initramfs-producer.mjs'
import {
  requireSameBytes,
  requireSameFile,
} from './fde-initramfs-providers.mjs'
import { replaceOnce, runSource } from './fde-reviewed-source.mjs'

/**
 * Admits copied native linker configuration and a cache regenerated from admitted providers.
 * Provider identity admission precedes this call; sources and the private candidate remain stable.
 * Source ldconfig parses admitted ELF data without executing or loading candidate code.
 */
export async function admitLinkerCache(root) {
  root = await realpath(root)
  await checkedDirectory(root, 'etc')
  try {
    await lstat(join(root, 'etc/ld.so.preload'))
    throw Error('Unsupported initramfs linker preload')
  } catch (error) {
    if (error.code !== 'ENOENT') throw error
  }
  if (!(await lstat(join(root, 'etc/ld.so.cache'))).isFile())
    throw Error('Unsupported linker cache file type')
  const producer = await readFile('/usr/sbin/mkinitramfs')
  requireNativeProducer(producer)
  const original = producer
    .toString()
    .match(/^cp -pPr \/etc\/ld\.so\.conf\* "\$DESTDIR"\/etc\/$/m)[0]
  const copyConfiguration = replaceOnce(
    original,
    '/etc/ld.so.conf*',
    '"$SOURCE_ROOT"/etc/ld.so.conf*',
  )
  const reference = await mkdtemp(join(tmpdir(), '.fde-linker-'))
  try {
    const image = join(reference, 'image')
    const configured = join(reference, 'configured')
    for (const directory of [image, configured])
      await mkdir(join(directory, 'etc'), { recursive: true })
    for (const [source, destination] of [
      ['/', image],
      [root, configured],
    ]) {
      await runSource('/bin/sh', [
        '-c',
        `SOURCE_ROOT="$1"\nDESTDIR="$2"\n${copyConfiguration}`,
        'fde-linker-configuration',
        source,
        destination,
      ])
    }
    const expectedFiles = (await filesUnder(image, 'etc')).sort()
    const actualFiles = (await filesUnder(configured, 'etc')).sort()
    if (JSON.stringify(actualFiles) !== JSON.stringify(expectedFiles))
      throw Error('Copied linker configuration files differ')
    for (const file of expectedFiles)
      await requireSameFile(join(configured, file), join(image, file))
    await runSource('/usr/bin/cp', [
      '--archive',
      '--reflink=auto',
      '--',
      join(root, 'usr'),
      image,
    ])
    await copyRootAliases(root, image)
    await mkdir(join(image, 'var/cache/ldconfig'), { recursive: true })
    const result = await runSource('/usr/sbin/ldconfig', ['-r', image])
    if (result.stderr.trim())
      throw Error(`Native linker cache diagnostics: ${result.stderr}`)
    await requireSameBytes(
      join(root, 'etc/ld.so.cache'),
      join(image, 'etc/ld.so.cache'),
    )
  } finally {
    await rm(reference, { recursive: true, force: true })
  }
}
