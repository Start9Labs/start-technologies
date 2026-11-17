import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { basename, dirname, join } from 'node:path'

import { checkedDirectory } from './fde-initramfs-paths.mjs'
import { harden } from './fde-initramfs-control.mjs'
import { admitBootstrapConfiguration } from './fde-initramfs-configuration.mjs'
import { admitNativeProviders } from './fde-initramfs-dependencies.mjs'
import { admitLinkerCache } from './fde-initramfs-linker.mjs'
import {
  admitKernelModules,
  admitRequiredBootModules,
} from './fde-initramfs-modules.mjs'

export const descriptorFile = (
  await readFile(new URL('./fde-descriptor-file', import.meta.url), 'utf8')
).trim()

/** Copies caller-owned public boot data before native archive capture. */
export async function installBootDescriptor(root, descriptor) {
  if (!descriptor.length) throw Error('Missing boot descriptor')
  const directory = dirname(descriptorFile.slice(1))
  await checkedDirectory(root, dirname(directory))
  try {
    await mkdir(join(root, directory), { mode: 0o755 })
  } catch (error) {
    if (error.code !== 'EEXIST') throw error
  }
  const parent = await checkedDirectory(root, directory)
  await writeFile(join(parent, basename(descriptorFile)), descriptor, {
    flag: 'wx',
    mode: 0o600,
  })
}

/** Source providers and the private candidate remain stable through preparation. */
export async function prepareGeneratedInitramfs(root, providerAliases) {
  await admitNativeProviders(root, async () => providerAliases)
  await admitKernelModules(root)
  await admitRequiredBootModules(root)
  const linkerFiles = await admitLinkerCache(root)
  await admitBootstrapConfiguration(root, linkerFiles)
  await harden(root)
}
