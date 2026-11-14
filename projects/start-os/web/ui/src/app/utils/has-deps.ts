import type { PackageDataEntry } from '../services/patch-db/data-model'
import { dependencyProvider } from './dependency-provider'

export function hasCurrentDeps(
  id: string,
  packages: Record<string, PackageDataEntry>,
): boolean {
  return Object.values(packages).some(owner =>
    Object.keys(owner.currentDependencies).some(
      logical => dependencyProvider(owner, logical) === id,
    ),
  )
}
