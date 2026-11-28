import {
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  readlink,
  realpath,
  rm,
} from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { checkedDirectory } from './fde-initramfs-paths.mjs'
import { filesystemTableCommands } from './fde-initramfs-producer.mjs'
import { requireSameFile } from './fde-initramfs-providers.mjs'
import { runSource } from './fde-reviewed-source.mjs'

/**
 * Admits public-bootstrap configuration, not the separately authorized private overlay.
 * The caller retains stable source/candidate paths and supplies linker-owned admitted paths.
 */
export async function admitBootstrapConfiguration(root, linkerFiles) {
  root = await realpath(root)
  await checkedDirectory(root, 'etc')
  const reference = await mkdtemp(join(tmpdir(), '.fde-configuration-'))
  try {
    await mkdir(join(reference, 'etc'))
    const producer = await readFile('/usr/sbin/mkinitramfs')
    await runSource('/bin/sh', [
      '-c',
      `DESTDIR="$1"\n${filesystemTableCommands(producer)}`,
      'fde-native-filesystem-tables',
      reference,
    ])
    const admitted = new Set(linkerFiles)
    for (const entry of await readdir(join(reference, 'etc'), {
      withFileTypes: true,
    })) {
      const path = join('etc', entry.name)
      if (entry.isFile())
        await requireSameFile(join(root, path), join(reference, path))
      else {
        if (
          !(await lstat(join(root, path))).isSymbolicLink() ||
          (await readlink(join(root, path))) !==
            (await readlink(join(reference, path)))
        )
          throw Error(`Generated filesystem table differs: ${path}`)
      }
      admitted.add(path)
    }
    async function walk(directory) {
      for (const entry of await readdir(
        await checkedDirectory(root, directory),
        { withFileTypes: true },
      )) {
        const path = join(directory, entry.name)
        if (entry.isDirectory()) await walk(path)
        else if (!admitted.has(path)) {
          if (!entry.isFile())
            throw Error(`Unsupported bootstrap configuration type: ${path}`)
          const source = join('/', path)
          if ((await realpath(source)) !== source)
            throw Error(`Redirected source configuration: ${path}`)
          await requireSameFile(join(root, path), source)
        }
      }
    }
    await walk('etc')
  } finally {
    await rm(reference, { recursive: true, force: true })
  }
}
