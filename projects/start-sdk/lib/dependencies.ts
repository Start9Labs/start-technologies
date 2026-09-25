import { VersionRange } from '@start9labs/start-core/exver'
import { checkDependencies } from '@start9labs/start-core/dependencies/dependencies'
import type {
  Effects,
  DependencyRequirement,
  Manifest,
  LocaleString,
} from '@start9labs/start-core/types'
import {
  runReactiveInit,
  setupOnInit,
} from '@start9labs/start-core/inits/setupInit'
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
  private readonly inits: { script: InitScript; taskIds: string[] }[] = []
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

  /** Appends an independent reactive init; declared replay IDs are cleared when disabled. */
  withInit(fn: InitScriptOrFn, taskReplayIds?: string[]) {
    if (this.optional && !taskReplayIds) {
      throw new Error(
        `Optional dependency ${this.id} must declare task replay IDs`,
      )
    }
    this.inits.push({ script: setupOnInit(fn), taskIds: taskReplayIds || [] })
    return this
  }

  private async isEnabled(effects: Effects) {
    return !this.enabledFn || (await this.enabledFn({ effects }))
  }

  initHandlers(): InitScript[] {
    return this.inits.map(
      ({ script, taskIds }): InitScript => ({
        init: async (effects, kind, progress) => {
          if (!(await this.isEnabled(effects))) {
            if (taskIds.length)
              await effects.action.clearTasks({ only: taskIds })
            return
          }
          await script.init(effects, kind, progress)
        },
      }),
    )
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

  async requirement(effects: Effects): Promise<DependencyRequirement | null> {
    if (!(await this.isEnabled(effects))) return null
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
        async child => {
          const requirement = await entry.requirement(child)
          if (deepEqual(active.get(entry.id) ?? null, requirement)) return
          if (requirement) active.set(entry.id, requirement)
          else active.delete(entry.id)
          if (!initializing) await publish()
        },
        kind,
        progress,
      )
      for (const [index, handler] of entry.initHandlers().entries()) {
        await runReactiveInit(
          effects,
          `dependency_${entry.id}_init_${index}`,
          handler,
          kind,
          progress,
        )
      }
    }
    initializing = false
    await publish()
  }

  /** Checks the active runtime requirements. */
  check(effects: Effects, packageIds?: Ids[]) {
    return checkDependencies<Ids>(effects, packageIds)
  }
}
