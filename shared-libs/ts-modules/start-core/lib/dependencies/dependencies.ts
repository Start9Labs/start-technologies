import { ExtendedVersion, VersionRange } from '../exver'
import {
  PackageId,
  HealthCheckId,
  DependencyRequirement,
  CheckDependenciesResult,
} from '../types'
import { Effects } from '../Effects'

export type CheckDependencies<DependencyId extends PackageId = PackageId> = {
  infoFor: (packageId: DependencyId) => {
    requirement: DependencyRequirement
    result: CheckDependenciesResult
  }

  installedSatisfied: (packageId: DependencyId) => boolean
  installedVersionSatisfied: (packageId: DependencyId) => boolean
  runningSatisfied: (packageId: DependencyId) => boolean
  tasksSatisfied: (packageId: DependencyId) => boolean
  healthCheckSatisfied: (
    packageId: DependencyId,
    healthCheckId?: HealthCheckId,
  ) => boolean
  satisfied: (packageId?: DependencyId) => boolean

  throwIfInstalledNotSatisfied: (packageId: DependencyId) => null
  throwIfInstalledVersionNotSatisfied: (packageId: DependencyId) => null
  throwIfRunningNotSatisfied: (packageId: DependencyId) => null
  throwIfTasksNotSatisfied: (packageId: DependencyId) => null
  throwIfHealthNotSatisfied: (
    packageId: DependencyId,
    healthCheckId?: HealthCheckId,
  ) => null
  throwIfNotSatisfied: (packageId?: DependencyId) => null
}
export async function checkDependencies<
  DependencyId extends PackageId = PackageId,
>(
  effects: Effects,
  packageIds?: DependencyId[],
): Promise<CheckDependencies<DependencyId>> {
  let [dependencies, results] = await Promise.all([
    effects.getDependencies(),
    effects.checkDependencies({
      packageIds,
    }),
  ])
  if (packageIds) {
    dependencies = dependencies.filter(d =>
      (packageIds as PackageId[]).includes(d.id),
    )
  }

  const infoFor = (packageId: DependencyId) => {
    const dependencyRequirement = dependencies.find(d => d.id === packageId)
    const dependencyResult = results.find(d => d.packageId === packageId)
    if (!dependencyRequirement || !dependencyResult) {
      throw new Error(`Unknown DependencyId ${packageId}`)
    }
    return { requirement: dependencyRequirement, result: dependencyResult }
  }

  const installedSatisfied = (packageId: DependencyId) =>
    !!infoFor(packageId).result.installedVersion
  const installedVersionSatisfied = (packageId: DependencyId) => {
    const dep = infoFor(packageId)
    if (!dep.result.installedVersion) return false
    return VersionRange.parse(dep.requirement.versionRange).satisfiedByRelease(
      [dep.result.installedVersion, ...dep.result.satisfies].map(v =>
        ExtendedVersion.parse(v),
      ),
    )
  }
  const runningSatisfied = (packageId: DependencyId) => {
    const dep = infoFor(packageId)
    return dep.requirement.kind !== 'running' || dep.result.isRunning
  }
  const tasksSatisfied = (packageId: DependencyId) =>
    Object.entries(infoFor(packageId).result.tasks).filter(
      ([_, t]) => t?.active && t.task.severity === 'critical',
    ).length === 0
  const healthCheckSatisfied = (
    packageId: DependencyId,
    healthCheckId?: HealthCheckId,
  ) => {
    const dep = infoFor(packageId)
    if (
      healthCheckId &&
      (dep.requirement.kind !== 'running' ||
        !dep.requirement.healthChecks.includes(healthCheckId))
    ) {
      throw new Error(`Unknown HealthCheckId ${healthCheckId}`)
    }
    const errors =
      dep.requirement.kind === 'running'
        ? dep.requirement.healthChecks
            .map(id => [id, dep.result.healthChecks[id] ?? null] as const)
            .filter(([id, _]) => (healthCheckId ? id === healthCheckId : true))
            .filter(([_, res]) => res?.result !== 'success')
        : []
    return errors.length === 0
  }
  const pkgSatisfied = (packageId: DependencyId) =>
    installedSatisfied(packageId) &&
    installedVersionSatisfied(packageId) &&
    runningSatisfied(packageId) &&
    tasksSatisfied(packageId) &&
    healthCheckSatisfied(packageId)
  const satisfied = (packageId?: DependencyId) =>
    packageId
      ? pkgSatisfied(packageId)
      : dependencies.every(d => pkgSatisfied(d.id as DependencyId))

  const throwIfInstalledNotSatisfied = (packageId: DependencyId) => {
    const dep = infoFor(packageId)
    if (!dep.result.installedVersion) {
      throw new Error(`${dep.result.title || packageId} is not installed`)
    }
    return null
  }
  const throwIfInstalledVersionNotSatisfied = (packageId: DependencyId) => {
    const dep = infoFor(packageId)
    if (!dep.result.installedVersion) {
      throw new Error(`${dep.result.title || packageId} is not installed`)
    }
    if (!installedVersionSatisfied(packageId)) {
      throw new Error(
        `Installed version ${dep.result.installedVersion} of ${dep.result.title || packageId} does not match expected version range ${dep.requirement.versionRange}`,
      )
    }
    return null
  }
  const throwIfRunningNotSatisfied = (packageId: DependencyId) => {
    const dep = infoFor(packageId)
    if (dep.requirement.kind === 'running' && !dep.result.isRunning) {
      throw new Error(`${dep.result.title || packageId} is not running`)
    }
    return null
  }
  const throwIfTasksNotSatisfied = (packageId: DependencyId) => {
    const dep = infoFor(packageId)
    const reqs = Object.entries(dep.result.tasks)
      .filter(([_, t]) => t?.active && t.task.severity === 'critical')
      .map(([id, _]) => id)
    if (reqs.length) {
      throw new Error(
        `The following action requests have not been fulfilled: ${reqs.join(', ')}`,
      )
    }
    return null
  }
  const throwIfHealthNotSatisfied = (
    packageId: DependencyId,
    healthCheckId?: HealthCheckId,
  ) => {
    const dep = infoFor(packageId)
    if (
      healthCheckId &&
      (dep.requirement.kind !== 'running' ||
        !dep.requirement.healthChecks.includes(healthCheckId))
    ) {
      throw new Error(`Unknown HealthCheckId ${healthCheckId}`)
    }
    const errors =
      dep.requirement.kind === 'running'
        ? dep.requirement.healthChecks
            .map(id => [id, dep.result.healthChecks[id] ?? null] as const)
            .filter(([id, _]) => (healthCheckId ? id === healthCheckId : true))
            .filter(([_, res]) => res?.result !== 'success')
        : []
    if (errors.length) {
      throw new Error(
        errors
          .map(([id, e]) =>
            e
              ? `Health Check ${e.name} of ${dep.result.title || packageId} failed with status ${e.result}${e.message ? `: ${e.message}` : ''}`
              : `Health Check ${id} of ${dep.result.title} does not exist`,
          )
          .join('; '),
      )
    }
    return null
  }
  const throwIfPkgNotSatisfied = (packageId: DependencyId) => {
    throwIfInstalledNotSatisfied(packageId)
    throwIfInstalledVersionNotSatisfied(packageId)
    throwIfRunningNotSatisfied(packageId)
    throwIfTasksNotSatisfied(packageId)
    throwIfHealthNotSatisfied(packageId)
    return null
  }
  const throwIfNotSatisfied = (packageId?: DependencyId) =>
    packageId
      ? throwIfPkgNotSatisfied(packageId)
      : (() => {
          const err = dependencies.flatMap(d => {
            try {
              throwIfPkgNotSatisfied(d.id as DependencyId)
            } catch (e) {
              if (e instanceof Error) return [e.message]
              throw e
            }
            return []
          })
          if (err.length) {
            throw new Error(err.join('; '))
          }
          return null
        })()

  return {
    infoFor,
    installedSatisfied,
    installedVersionSatisfied,
    runningSatisfied,
    tasksSatisfied,
    healthCheckSatisfied,
    satisfied,
    throwIfInstalledNotSatisfied,
    throwIfInstalledVersionNotSatisfied,
    throwIfRunningNotSatisfied,
    throwIfTasksNotSatisfied,
    throwIfHealthNotSatisfied,
    throwIfNotSatisfied,
  }
}
