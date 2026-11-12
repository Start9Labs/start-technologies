import { inject, Injectable } from '@angular/core'
import { T } from '@start9labs/start-core'
import deepEqual from 'fast-deep-equal'
import { PatchDB } from 'patch-db-client'
import { Observable } from 'rxjs'
import { distinctUntilChanged, map, shareReplay } from 'rxjs/operators'
import { dependencyProvider } from '../utils/dependency-provider'
import { isInstalled } from '../utils/get-package-data'
import {
  DataModel,
  InstalledState,
  PackageDataEntry,
} from './patch-db/data-model'
import { getInstalledBaseStatus } from './pkg-status-rendering.service'

export type AllDependencyErrors = Record<string, PkgDependencyErrors>
export type PkgDependencyErrors = Record<string, DependencyError | null>

export type DependencyError =
  | { type: 'selectionRequired' }
  | { type: 'noCompatibleProvider' }
  | DependencyErrorNotInstalled
  | DependencyErrorNotRunning
  | DependencyErrorTaskRequired
  | DependencyErrorHealthChecksFailed
  | DependencyErrorTransitive

export type DependencyErrorNotInstalled = { type: 'notInstalled' }
export type DependencyErrorNotRunning = { type: 'notRunning' }
export type DependencyErrorTaskRequired = { type: 'taskRequired' }
export type DependencyErrorHealthChecksFailed = {
  type: 'healthChecksFailed'
  check?: T.NamedHealthCheckResult
}
export type DependencyErrorTransitive = { type: 'transitive' }

@Injectable({ providedIn: 'root' })
export class DepErrorService {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  readonly depErrors$: Observable<AllDependencyErrors> = this.patch
    .watch$('packageData')
    .pipe(
      map(pkgs => {
        const errors = Object.fromEntries(
          Object.keys(pkgs).map(id => [id, this.getDepErrors(pkgs, id)]),
        )
        return Object.fromEntries(
          Object.entries(errors).map(([id, deps]) => [
            id,
            Object.fromEntries(
              Object.entries(deps).map(([logical, error]) => [
                logical,
                error ||
                  (hasTransitiveError(pkgs, errors, id, logical)
                    ? { type: 'transitive' as const }
                    : null),
              ]),
            ),
          ]),
        )
      }),
      distinctUntilChanged(deepEqual),
      shareReplay(1),
    )

  getPkgDepErrors$(pkgId: string): Observable<PkgDependencyErrors> {
    return this.depErrors$.pipe(
      map(depErrors => depErrors[pkgId]),
      distinctUntilChanged(deepEqual),
    )
  }

  private getDepErrors(
    pkgs: DataModel['packageData'],
    pkgId: string,
  ): PkgDependencyErrors {
    const pkg = pkgs[pkgId]
    if (!pkg || !isInstalled(pkg)) return {}

    return Object.fromEntries(
      Object.keys(pkg.currentDependencies)
        .filter(id => id !== pkgId)
        .map(depId => [depId, this.getDepError(pkgs, pkg, depId)]),
    )
  }

  private getDepError(
    pkgs: DataModel['packageData'],
    pkg: PackageDataEntry<InstalledState>,
    depId: string,
  ): DependencyError | null {
    const binding = pkg.dependencyBindings?.[depId]
    if (!binding) return null
    if (!binding.providerId && Object.keys(binding.candidates).length > 1) {
      return { type: 'selectionRequired' }
    }
    if (!binding.providerId) return { type: 'noCompatibleProvider' }
    const providerId = dependencyProvider(pkg, depId)
    const dep = providerId ? pkgs[providerId] : undefined
    if (
      !dep ||
      (dep.stateInfo.state !== 'installed' &&
        dep.stateInfo.state !== 'updating')
    ) {
      return { type: 'notInstalled' }
    }

    const currentDep = pkg.currentDependencies[depId]
    if (
      Object.values(pkg.tasks).some(
        t =>
          t?.active &&
          t.task.packageId === depId &&
          t.task.severity === 'critical',
      )
    ) {
      return { type: 'taskRequired' }
    }

    const depStatus = getInstalledBaseStatus(dep.statusInfo)
    if (currentDep?.kind === 'running') {
      if (depStatus !== 'running' && depStatus !== 'starting') {
        return { type: 'notRunning' }
      }
      for (const id of currentDep.healthChecks) {
        const check = dep.statusInfo.health[id]
        if (check?.result !== 'success') {
          return { type: 'healthChecksFailed', check }
        }
      }
    }
    return null
  }
}

function hasTransitiveError(
  packages: DataModel['packageData'],
  errors: AllDependencyErrors,
  ownerId: string,
  logical: string,
  visited = new Set<string>(),
): boolean {
  const owner = packages[ownerId]
  const provider = owner && dependencyProvider(owner, logical)
  if (!provider || visited.has(provider)) return false
  visited.add(provider)
  return Object.entries(errors[provider] || {}).some(
    ([depId, error]) =>
      !!error || hasTransitiveError(packages, errors, provider, depId, visited),
  )
}
