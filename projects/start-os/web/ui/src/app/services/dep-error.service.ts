import { inject, Injectable } from '@angular/core'
import { Exver } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import deepEqual from 'fast-deep-equal'
import { PatchDB } from 'patch-db-client'
import { Observable } from 'rxjs'
import { distinctUntilChanged, map, shareReplay } from 'rxjs/operators'
import { isInstalled } from 'src/app/utils/get-package-data'
import {
  DataModel,
  InstalledState,
  PackageDataEntry,
} from './patch-db/data-model'
import { getInstalledBaseStatus } from './pkg-status-rendering.service'

export type AllDependencyErrors = Record<string, PkgDependencyErrors>
export type PkgDependencyErrors = Record<string, DependencyError | null>

export type DependencyError =
  | DependencyErrorNotInstalled
  | DependencyErrorNotRunning
  | DependencyErrorIncorrectVersion
  | DependencyErrorTaskRequired
  | DependencyErrorHealthChecksFailed
  | DependencyErrorTransitive

export type DependencyErrorNotInstalled = {
  type: 'notInstalled'
}

export type DependencyErrorNotRunning = {
  type: 'notRunning'
}

export type DependencyErrorIncorrectVersion = {
  type: 'incorrectVersion'
  expected: string // version range
  received: string // version
}

export interface DependencyErrorTaskRequired {
  type: 'taskRequired'
}

export type DependencyErrorHealthChecksFailed = {
  type: 'healthChecksFailed'
  check?: T.NamedHealthCheckResult
}

export type DependencyErrorTransitive = {
  type: 'transitive'
}

@Injectable({
  providedIn: 'root',
})
export class DepErrorService {
  private readonly exver = inject(Exver)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)

  readonly depErrors$: Observable<AllDependencyErrors> = this.patch
    .watch$('packageData')
    .pipe(
      map(pkgs =>
        Object.keys(pkgs)
          .map(id => ({
            id,
            depth: dependencyDepth(pkgs, id),
          }))
          .sort((a, b) => (b.depth > a.depth ? -1 : 1))
          .reduce(
            (errors, { id }): AllDependencyErrors => ({
              ...errors,
              [id]: this.getDepErrors(pkgs, id, errors),
            }),
            {} as AllDependencyErrors,
          ),
      ),
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
    outerErrors: AllDependencyErrors,
  ): PkgDependencyErrors {
    const pkg = pkgs[pkgId]

    if (!pkg || !isInstalled(pkg)) return {}

    return currentDeps(pkgs, pkgId).reduce(
      (innerErrors, depId): PkgDependencyErrors => ({
        ...innerErrors,
        [depId]: this.getDepError(pkgs, pkg, depId, outerErrors),
      }),
      {} as PkgDependencyErrors,
    )
  }

  private getDepError(
    pkgs: DataModel['packageData'],
    pkg: PackageDataEntry<InstalledState>,
    depId: string,
    outerErrors: AllDependencyErrors,
  ): DependencyError | null {
    const dep = pkgs[depId]

    // not installed
    if (!dep || dep.stateInfo.state !== 'installed') {
      return {
        type: 'notInstalled',
      }
    }

    const currentDep = pkg.currentDependencies[depId]
    const depManifest = dep.stateInfo.manifest
    const expected = currentDep?.versionRange || ''

    // incorrect version
    if (
      !this.exver.releaseSatisfies(
        depManifest.version,
        depManifest.satisfies,
        expected,
      )
    ) {
      return {
        expected,
        type: 'incorrectVersion',
        received: depManifest.version,
      }
    }

    // action required
    if (
      Object.values(pkg.tasks)
        .filter(t => !!t)
        .some(
          t =>
            t.active &&
            t.task.packageId === depId &&
            t.task.severity === 'critical',
        )
    ) {
      return {
        type: 'taskRequired',
      }
    }

    const depStatus = getInstalledBaseStatus(dep.statusInfo)

    if (currentDep?.kind === 'running') {
      // not running
      if (depStatus !== 'running' && depStatus !== 'starting') {
        return {
          type: 'notRunning',
        }
      }
      // health check failure
      for (let id of currentDep.healthChecks) {
        const check = dep.statusInfo.health[id]
        if (check?.result !== 'success') {
          return {
            type: 'healthChecksFailed',
            check,
          }
        }
      }
    }

    // transitive
    const transitiveError = currentDeps(pkgs, depId).some(transitiveId =>
      Object.values(outerErrors[transitiveId] || {}).some(err => !!err),
    )

    if (transitiveError) {
      return {
        type: 'transitive',
      }
    }

    return null
  }
}

function currentDeps(pkgs: DataModel['packageData'], id: string): string[] {
  return Object.keys(pkgs[id]?.currentDependencies || {}).filter(
    depId => depId !== id,
  )
}

function dependencyDepth(
  pkgs: DataModel['packageData'],
  id: string,
  depth = 0,
): number {
  return currentDeps(pkgs, id).reduce(
    (prev, depId) => dependencyDepth(pkgs, depId, prev + 1),
    depth,
  )
}
