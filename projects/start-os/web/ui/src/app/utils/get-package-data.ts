import { indexedEntries } from '@start9labs/shared'
import { PatchDB } from 'patch-db-client'
import {
  DataModel,
  InstalledState,
  InstallingState,
  PackageDataEntry,
  UpdatingState,
} from 'src/app/services/patch-db/data-model'
import { firstValueFrom } from 'rxjs'
import { T } from '@start9labs/start-core'

export async function getPackage(
  patch: PatchDB<DataModel>,
  id: string,
): Promise<PackageDataEntry | undefined> {
  return firstValueFrom(patch.watch$('packageData', id))
}

export async function getAllPackages(
  patch: PatchDB<DataModel>,
): Promise<DataModel['packageData']> {
  return firstValueFrom(patch.watch$('packageData'))
}

export function getManifest(pkg: PackageDataEntry): T.Manifest {
  return isInstalling(pkg) || isRestoring(pkg) || isUpdating(pkg)
    ? pkg.stateInfo.installingInfo.newManifest
    : pkg.stateInfo.manifest!
}

export function byTitle(a: PackageDataEntry, b: PackageDataEntry): number {
  return getManifest(a).title.localeCompare(getManifest(b).title)
}

export function getCurrentManifest(pkg: PackageDataEntry): T.Manifest {
  return pkg.stateInfo.manifest ?? getManifest(pkg)
}

/** Tasks on the package itself or a current dependency, keyed by replay ID, in creation order. */
export function getLiveTasks(pkg: PackageDataEntry): [string, T.TaskEntry][] {
  const { id } = getManifest(pkg)
  return indexedEntries(pkg.tasks).filter(
    ([, entry]) =>
      entry.task.packageId === id ||
      entry.task.packageId in pkg.currentDependencies,
  )
}

export function isInstalled(
  pkg: PackageDataEntry,
): pkg is PackageDataEntry<InstalledState> {
  return pkg.stateInfo.state === 'installed'
}

export function isRemoving(
  pkg: PackageDataEntry,
): pkg is PackageDataEntry<InstalledState> {
  return pkg.stateInfo.state === 'removing'
}

export function isInstalling(
  pkg: T.PackageDataEntry,
): pkg is PackageDataEntry<InstallingState> {
  return pkg.stateInfo.state === 'installing'
}

export function isRestoring(
  pkg: PackageDataEntry,
): pkg is PackageDataEntry<InstallingState> {
  return pkg.stateInfo.state === 'restoring'
}

export function isUpdating(
  pkg: PackageDataEntry,
): pkg is PackageDataEntry<UpdatingState> {
  return pkg.stateInfo.state === 'updating'
}
