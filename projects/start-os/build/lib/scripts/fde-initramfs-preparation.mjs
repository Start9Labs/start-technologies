import { harden } from './fde-initramfs-control.mjs'
import { admitNativeProviders } from './fde-initramfs-dependencies.mjs'
import { admitLinkerCache } from './fde-initramfs-linker.mjs'
import {
  admitKernelModules,
  admitRequiredBootModules,
} from './fde-initramfs-modules.mjs'

/** Source providers and the private candidate remain stable through preparation. */
export async function prepareGeneratedInitramfs(root, providerAliases) {
  await admitNativeProviders(root, async () => providerAliases)
  await admitKernelModules(root)
  await admitRequiredBootModules(root)
  await admitLinkerCache(root)
  await harden(root)
}
