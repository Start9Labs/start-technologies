import { ExtendedVersion, VersionRange } from '@start9labs/start-core/exver'
import * as T from '@start9labs/start-core/types'
import {
  InitFn,
  InitKind,
  InitScript,
  InitScriptOrFn,
  UninitFn,
  UninitScript,
  UninitScriptOrFn,
} from '@start9labs/start-core/inits'
import { FullProgressTracker } from '@start9labs/start-core/util/FullProgressTracker'
import { Graph, Vertex, once } from '../util'
import { IMPOSSIBLE, MigrationOpts, VersionInfo } from './VersionInfo'

/**
 * Read the current data version from the effects system.
 * @param effects - The effects context
 * @returns The parsed ExtendedVersion or VersionRange, or null if no version is set
 */
export async function getDataVersion(effects: T.Effects) {
  const versionStr = await effects.getDataVersion()
  if (!versionStr) return null
  try {
    return ExtendedVersion.parse(versionStr)
  } catch (_) {
    return VersionRange.parse(versionStr)
  }
}

/**
 * Persist a data version to the effects system.
 * @param effects - The effects context
 * @param version - The version to set, or null to clear it
 */
export async function setDataVersion(
  effects: T.Effects,
  version: ExtendedVersion | VersionRange | null,
) {
  return effects.setDataVersion({ version: version?.toString() || null })
}

function isExver(v: ExtendedVersion | VersionRange): v is ExtendedVersion {
  return 'satisfies' in v
}

function isRange(v: ExtendedVersion | VersionRange): v is VersionRange {
  return 'satisfiedBy' in v
}

/**
 * Check whether two version specifiers overlap (i.e. share at least one common version).
 * Works with any combination of ExtendedVersion and VersionRange.
 *
 * @param a - First version or range
 * @param b - Second version or range
 * @returns True if the two specifiers overlap
 */
export function overlaps(
  a: ExtendedVersion | VersionRange,
  b: ExtendedVersion | VersionRange,
) {
  return (
    (isRange(a) && isRange(b) && a.intersects(b)) ||
    (isRange(a) && isExver(b) && a.satisfiedBy(b)) ||
    (isExver(a) && isRange(b) && a.satisfies(b)) ||
    (isExver(a) && isExver(b) && a.equals(b))
  )
}

/**
 * A directed graph of service versions and their migration paths.
 *
 * Builds a graph from {@link VersionInfo} definitions, then uses shortest-path
 * search to find and execute migration sequences between any two versions.
 * Implements both {@link InitScript} (for install/update migrations) and
 * {@link UninitScript} (for uninstall/downgrade migrations).
 *
 * @typeParam CurrentVersion - The string literal type of the current service version
 */
export class VersionGraph<CurrentVersion extends string>
  implements InitScript, UninitScript
{
  protected initFn = this.init.bind(this)
  protected uninitFn = this.uninit.bind(this)
  private readonly graph: () => Graph<
    ExtendedVersion | VersionRange,
    ((opts: MigrationOpts) => Promise<void>) | undefined
  >
  /** Dump the version graph as a human-readable string for debugging */
  dump(): string {
    return this.graph().dump(metadata => metadata?.toString())
  }
  private constructor(
    readonly current: VersionInfo<CurrentVersion>,
    versions: Array<VersionInfo<any>>,
  ) {
    this.graph = once(() => {
      const graph = new Graph<
        ExtendedVersion | VersionRange,
        ((opts: MigrationOpts) => Promise<void>) | undefined
      >()
      const flavorMap: Record<
        string,
        [
          ExtendedVersion,
          VersionInfo<any>,
          Vertex<
            ExtendedVersion | VersionRange,
            ((opts: MigrationOpts) => Promise<void>) | undefined
          >,
        ][]
      > = {}
      for (let version of [current, ...versions]) {
        const v = ExtendedVersion.parse(version.options.version)
        const vertex = graph.addVertex(v, [], [])
        const flavor = v.flavor || ''
        if (!flavorMap[flavor]) {
          flavorMap[flavor] = []
        }
        flavorMap[flavor].push([v, version, vertex])
      }
      for (let flavor in flavorMap) {
        flavorMap[flavor].sort((a, b) => a[0].compareForSort(b[0]))
        let prev:
          | [
              ExtendedVersion,
              VersionInfo<any>,
              Vertex<
                ExtendedVersion | VersionRange,
                ((opts: MigrationOpts) => Promise<void>) | undefined
              >,
            ]
          | undefined = undefined
        for (let [v, version, vertex] of flavorMap[flavor]) {
          if (version.options.migrations.up !== IMPOSSIBLE) {
            let range
            if (prev) {
              graph.addEdge(version.options.migrations.up, prev[2], vertex)
              range = VersionRange.anchor('>=', prev[0]).and(
                VersionRange.anchor('<', v),
              )
            } else {
              range = VersionRange.anchor('<', v)
            }
            const vRange = graph.addVertex(range, [], [])
            graph.addEdge(version.options.migrations.up, vRange, vertex)
          }

          if (version.options.migrations.down !== IMPOSSIBLE) {
            let range
            if (prev) {
              graph.addEdge(version.options.migrations.down, vertex, prev[2])
              range = VersionRange.anchor('>=', prev[0]).and(
                VersionRange.anchor('<', v),
              )
            } else {
              range = VersionRange.anchor('<', v)
            }
            const vRange = graph.addVertex(range, [], [])
            graph.addEdge(version.options.migrations.down, vertex, vRange)
          }

          if (version.options.migrations.other) {
            for (let rangeStr in version.options.migrations.other) {
              const range = VersionRange.parse(rangeStr)
              const vRange = graph.addVertex(range, [], [])
              const migration = version.options.migrations.other[rangeStr]
              if (migration.up) graph.addEdge(migration.up, vRange, vertex)
              if (migration.down) graph.addEdge(migration.down, vertex, vRange)
              for (let matching of graph.findVertex(
                v => isExver(v.metadata) && v.metadata.satisfies(range),
              )) {
                if (migration.up) graph.addEdge(migration.up, matching, vertex)
                if (migration.down)
                  graph.addEdge(migration.down, vertex, matching)
              }
            }
          }

          prev = [v, version, vertex]
        }
      }
      return graph
    })
  }
  currentVersion = once(() =>
    ExtendedVersion.parse(this.current.options.version),
  )
  /**
   * Each exported `VersionInfo.of()` should be imported and provided as an argument to this function.
   *
   * ** The current version must be the FIRST argument. **
   */
  static of<
    CurrentVersion extends string,
    OtherVersions extends Array<VersionInfo<any>>,
  >(options: { current: VersionInfo<CurrentVersion>; other: OtherVersions }) {
    return new VersionGraph(options.current, options.other)
  }
  /**
   * Execute the shortest migration path between two versions.
   *
   * Finds the shortest path in the version graph from `from` to `to`,
   * executes each migration step in order, and updates the data version after each step.
   *
   * @param options.effects - The effects context
   * @param options.from - The source version or range
   * @param options.to - The target version or range
   * @returns The final data version after migration
   * @throws If no migration path exists between the two versions
   */
  async migrate({
    effects,
    from,
    to,
    progress,
  }: {
    effects: T.Effects
    from: ExtendedVersion | VersionRange
    to: ExtendedVersion | VersionRange
    progress?: FullProgressTracker
  }): Promise<ExtendedVersion | VersionRange> {
    if (overlaps(from, to)) return from
    // Each migration step gets a FullProgressTracker. When invoked without one
    // (tests, direct calls) fall back to a sink-less tracker that no-ops sync.
    const reportProgress = progress ?? new FullProgressTracker()
    const graph = this.graph()
    if (from && to) {
      const path = graph.shortestPath(
        v => overlaps(v.metadata, from),
        v => overlaps(v.metadata, to),
      )
      if (path) {
        console.log(
          `Migrating ${
            path.reduce<{ acc: string; prev: string | null }>(
              ({ acc, prev }, x) => ({
                acc:
                  acc +
                  (prev && prev != x.from.metadata.toString()
                    ? ` (as ${prev})`
                    : '') +
                  ' -> ' +
                  x.to.metadata.toString(),
                prev: x.to.metadata.toString(),
              }),
              { acc: from.toString(), prev: null },
            ).acc
          }`,
        )
        let dataVersion = from
        for (let edge of path) {
          if (edge.metadata) {
            await edge.metadata({ effects, progress: reportProgress })
          }
          dataVersion = edge.to.metadata
          await setDataVersion(effects, edge.to.metadata)
        }
        return dataVersion
      }
    }
    throw new Error(
      `this service has no migration path from ${from.toString()} to ${to.toString()}, so its data cannot be carried over`,
    )
  }
  /**
   * Compute the version range from which the current version can be reached via migration.
   * Uses reverse breadth-first search from the current version vertex.
   */
  canMigrateFrom = once(() =>
    Array.from(
      this.graph().reverseBreadthFirstSearch(v =>
        overlaps(v.metadata, this.currentVersion()),
      ),
    )
      .reduce(
        (acc, x) =>
          acc.or(
            isRange(x.metadata)
              ? x.metadata
              : VersionRange.anchor('=', x.metadata),
          ),
        VersionRange.none(),
      )
      .normalize(),
  )
  /**
   * Compute the version range that the current version can migrate to.
   * Uses forward breadth-first search from the current version vertex.
   */
  canMigrateTo = once(() =>
    Array.from(
      this.graph().breadthFirstSearch(v =>
        overlaps(v.metadata, this.currentVersion()),
      ),
    )
      .reduce(
        (acc, x) =>
          acc.or(
            isRange(x.metadata)
              ? x.metadata
              : VersionRange.anchor('=', x.metadata),
          ),
        VersionRange.none(),
      )
      .normalize(),
  )

  /**
   * InitScript implementation: migrate from the stored data version to the current version.
   * If no data version exists (fresh install), sets it to the current version.
   * @param effects - The effects context
   * @param _kind - The init kind (unused; migration is decided by data version)
   * @param progress - Tracker the harness threads into each migration step
   */
  async init(
    effects: T.Effects,
    _kind?: InitKind,
    progress?: FullProgressTracker,
  ): Promise<void> {
    const from = await getDataVersion(effects)
    if (from) {
      await this.migrate({
        effects,
        from,
        to: this.currentVersion(),
        progress,
      })
    } else {
      await effects.setDataVersion({ version: this.current.options.version })
    }
  }

  /**
   * UninitScript implementation: migrate from the current data version to the target version.
   * Used during uninstall or downgrade to prepare data for the target version.
   *
   * @param effects - The effects context
   * @param target - The target version to migrate to, or null to clear the data version
   */
  async uninit(
    effects: T.Effects,
    target: VersionRange | ExtendedVersion | null,
  ): Promise<void> {
    if (target) {
      if (isRange(target) && !target.satisfiable()) {
        throw new Error(
          `this service has no migration path to the version being installed, so its data cannot be carried over`,
        )
      }
      const from = await getDataVersion(effects)
      if (from) {
        target = await this.migrate({
          effects,
          from,
          to: target,
        })
      }
    }
    await setDataVersion(effects, target)
  }
}

// prettier-ignore
export type EnsureUniqueId<A, B = A, OtherVersions = never> =
  B extends [] ? A : 
  B extends [VersionInfo<infer Version>, ...infer Rest] ? (
    Version extends OtherVersions ?  "One or more versions are not unique"[] :
    EnsureUniqueId<A, Rest, Version | OtherVersions>
  ) : "There exists a migration that is not a Migration"[]
