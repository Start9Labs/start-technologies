import { VersionRange } from '@start9labs/start-core/exver'
import { checkDependencies } from '@start9labs/start-core/dependencies/dependencies'
import type {
  Effects,
  DependencyRequirement,
  Manifest,
  LocaleString,
} from '@start9labs/start-core/types'
import { runReactiveInit } from '@start9labs/start-core/inits/setupInit'
import type {
  InitKind,
  InitScript,
  InitScriptOrFn,
} from '@start9labs/start-core/inits/setupInit'
import { deepEqual } from '@start9labs/start-core/util/deepEqual'
import type { FullProgressTracker } from '@start9labs/start-core/util/FullProgressTracker'

type Base = {
  description: LocaleString | null
  metadata: { title: LocaleString; icon: string }
  versionRange: string
} & (
  | { kind: 'running'; healthChecks: string[] }
  | { kind: 'exists'; healthChecks?: never }
)

type Narrowing = {
  versionRange?: string
  kind?: 'running'
  healthChecks?: string[]
}

/** A published base requirement with optional reactive runtime restrictions. */
export class Dependency<Id extends string = string> {
  private narrowing?: (options: {
    effects: Effects
  }) => Promise<Narrowing | null>
  private initFn?: InitScriptOrFn
  private taskIds: string[] = []
  private enabledFn?: (options: { effects: Effects }) => Promise<boolean>

  private constructor(
    readonly id: Id,
    readonly optional: boolean,
    readonly base: Base,
  ) {
    VersionRange.parse(base.versionRange)
  }

  /** Publishes a requirement that is always active. */
  static required<const Id extends string>(id: Id, base: Base) {
    return new Dependency(id, false, base)
  }

  /** Publishes a requirement activated by the service's configuration. */
  static optional<const Id extends string>(
    id: Id,
    base: Base & {
      enabled: (options: { effects: Effects }) => Promise<boolean>
    },
  ) {
    const { enabled, ...fields } = base
    const dependency = new Dependency(id, true, fields)
    dependency.enabledFn = enabled
    return dependency
  }

  /** Restricts the published requirement for the active configuration. */
  withDynamicNarrowing(
    fn: (options: { effects: Effects }) => Promise<Narrowing | null>,
  ) {
    this.narrowing = fn
    return this
  }

  /** Runs while enabled; declared replay IDs are cleared when disabled. */
  withInit(fn: InitScriptOrFn, taskReplayIds?: string[]) {
    if (this.optional && !taskReplayIds) {
      throw new Error(
        `Optional dependency ${this.id} must declare task replay IDs`,
      )
    }
    this.initFn = fn
    this.taskIds = taskReplayIds || []
    return this
  }

  manifestInfo(): Manifest['dependencies'][string] {
    return {
      description: this.base.description,
      optional: this.optional,
      versionRange: this.base.versionRange,
      kind: this.base.kind,
      ...(this.base.kind === 'running'
        ? { healthChecks: this.base.healthChecks }
        : {}),
      metadata: this.base.metadata,
    }
  }

  async sync(
    effects: Effects,
    initKind: InitKind,
    progress?: FullProgressTracker,
  ): Promise<DependencyRequirement | null> {
    if (this.enabledFn && !(await this.enabledFn({ effects }))) {
      if (this.taskIds.length)
        await effects.action.clearTasks({ only: this.taskIds })
      return null
    }
    const narrowed = await this.narrowing?.({ effects })
    const baseRange = VersionRange.parse(this.base.versionRange)
    const runtimeRange = narrowed?.versionRange
      ? VersionRange.parse(narrowed.versionRange)
      : baseRange
    if (!baseRange.intersects(runtimeRange)) {
      throw new Error(`Dependency ${this.id} has an incompatible version range`)
    }
    if (
      narrowed?.healthChecks?.length &&
      this.base.kind === 'exists' &&
      narrowed.kind !== 'running'
    ) {
      throw new Error(
        `Dependency ${this.id} has health checks without a running requirement`,
      )
    }
    const kind =
      this.base.kind === 'running' || narrowed?.kind === 'running'
        ? 'running'
        : 'exists'
    const versionRange = narrowed?.versionRange
      ? baseRange.and(runtimeRange).toString()
      : this.base.versionRange
    const requirement: DependencyRequirement =
      kind === 'running'
        ? {
            id: this.id,
            kind,
            versionRange,
            healthChecks: [
              ...(this.base.kind === 'running' ? this.base.healthChecks : []),
              ...(narrowed?.healthChecks || []),
            ],
          }
        : { id: this.id, kind, versionRange }
    if (this.initFn) {
      if ('init' in this.initFn)
        await this.initFn.init(effects, initKind, progress)
      else await this.initFn(effects, initKind, progress!)
    }
    return requirement
  }
}

/** A single dependency definition for the manifest and runtime. */
export class Dependencies<Ids extends string = never> implements InitScript {
  private constructor(private readonly entries: Dependency[]) {}

  static of() {
    return new Dependencies([])
  }

  addDependency<const Id extends string>(
    dependency: Dependency<Id>,
  ): Dependencies<Ids | Id> {
    if (this.entries.some(entry => entry.id === dependency.id)) {
      throw new Error(`Duplicate dependency ${dependency.id}`)
    }
    return new Dependencies<Ids | Id>([...this.entries, dependency])
  }

  /** The dependency metadata embedded in the package manifest. */
  manifestDependencies(): Manifest['dependencies'] {
    return Object.fromEntries(
      this.entries.map(entry => [entry.id, entry.manifestInfo()]),
    )
  }

  async init(
    effects: Effects,
    kind: InitKind = null,
    progress?: FullProgressTracker,
  ): Promise<void> {
    const active = new Map<string, DependencyRequirement>()
    let initializing = true
    let lastPublish = Promise.resolve()
    const publish = () => {
      const dependencies = this.entries.flatMap(entry => {
        const requirement = active.get(entry.id)
        return requirement ? [requirement] : []
      })
      const next = lastPublish.then(() =>
        effects.setDependencies({ dependencies }),
      )
      lastPublish = next.then(
        () => {},
        () => {},
      )
      return next
    }
    for (const entry of this.entries) {
      await runReactiveInit(
        effects,
        `dependency_${entry.id}`,
        async (child, runKind, runProgress) => {
          const requirement = await entry.sync(child, runKind, runProgress)
          if (deepEqual(active.get(entry.id) ?? null, requirement)) return
          if (requirement) active.set(entry.id, requirement)
          else active.delete(entry.id)
          if (!initializing) await publish()
        },
        kind,
        progress,
      )
    }
    initializing = false
    await publish()
  }

  /** Checks the active runtime requirements. */
  check(effects: Effects, packageIds?: Ids[]) {
    return checkDependencies<Ids>(effects, packageIds)
  }
}
