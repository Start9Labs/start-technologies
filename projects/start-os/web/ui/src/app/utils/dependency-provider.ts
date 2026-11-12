import type { PackageDataEntry } from '../services/patch-db/data-model'

/** Dependency targets follow owner bindings; self and administration remain physical. */
export function dependencyProvider(
  owner: PackageDataEntry,
  target: string,
): string | null {
  const self =
    owner.stateInfo.manifest?.id ||
    owner.stateInfo.installingInfo?.newManifest.id
  if (target === self || target === 'start-os') return target
  if (target in owner.currentDependencies) {
    return owner.dependencyBindings?.[target]?.providerId ?? null
  }
  return target
}
