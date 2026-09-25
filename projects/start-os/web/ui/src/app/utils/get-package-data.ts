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

/** Tasks on the package itself or a current dependency, keyed by replay ID. */
export function getLiveTasks(pkg: PackageDataEntry): [string, T.TaskEntry][] {
  const { id } = getManifest(pkg)
  return Object.entries(pkg.tasks).filter(
    (entry): entry is [string, T.TaskEntry] =>
      !!entry[1] &&
      (entry[1].task.packageId === id ||
        entry[1].task.packageId in pkg.currentDependencies),
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
