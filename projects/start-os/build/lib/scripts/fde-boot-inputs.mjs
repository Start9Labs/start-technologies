import { lstat } from 'node:fs/promises'
import { join } from 'node:path'

import { checkedDirectory, kernelDirectory } from './fde-initramfs-paths.mjs'

/** Requires a stable authenticated public source root. */
export async function bootProviders(root) {
  const { version } = await kernelDirectory(root)
  const kernel = join(
    await checkedDirectory(root, 'boot'),
    `vmlinuz-${version}`,
  )
  const osRelease = join(await checkedDirectory(root, 'usr/lib'), 'os-release')
  for (const path of [kernel, osRelease]) {
    if (!(await lstat(path)).isFile())
      throw Error(`Boot provider is not a regular source file: ${path}`)
  }
  return { kernel, osRelease }
}
