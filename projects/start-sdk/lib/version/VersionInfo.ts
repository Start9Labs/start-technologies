import { ValidateExVer, ValidateExVerRange } from '@start9labs/start-core/exver'
import * as T from '@start9labs/start-core/types'
import { FullProgressTracker } from '@start9labs/start-core/util/FullProgressTracker'

/**
 * Sentinel value indicating that a migration in a given direction is not possible.
 * Use this for `migrations.up` or `migrations.down` to prevent migration.
 */
export const IMPOSSIBLE: unique symbol = Symbol('IMPOSSIBLE')

/**
 * Opts passed to a migration. `progress` is a {@link FullProgressTracker} for
 * this migration step — add a phase and update it to show progress in the
 * update UI; updates auto-report in the background. Safe to ignore for quick
 * migrations.
 */
export type MigrationOpts = {
  effects: T.Effects
  progress: FullProgressTracker
}

type Migration = {
  up?: (opts: MigrationOpts) => Promise<void>
  down?: (opts: MigrationOpts) => Promise<void>
}

/**
 * Validates each key of a `migrations.other` record as an exver version range.
 * An invalid key collapses that entry's value type to `never`, surfacing a
 * compile error at the offending entry. (The `[…] extends […]` tuple wrapping
 * prevents distribution over `never`.)
 */
type ValidateOtherKeys<O> = {
  [K in keyof O]: K extends string
    ? [ValidateExVerRange<K>] extends [never]
      ? never
      : O[K]
    : O[K]
}

/**
 * Configuration options for a single service version definition.
 *
 * @typeParam Version - The string literal exver version number
 */
export type VersionOptions<Version extends string> = {
  /** The exver-compliant version number */
  version: Version & ValidateExVer<Version>
  /** The release notes for this version */
  releaseNotes: T.LocaleString
  /** Data migrations for this version */
  migrations: {
    /**
     * A migration from the previous version. Leave empty to indicate no migration is necessary.
     * Set to `IMPOSSIBLE` to indicate migrating from the previous version is not possible.
     */
    up?: ((opts: MigrationOpts) => Promise<void>) | typeof IMPOSSIBLE
    /**
     * A migration to the previous version. Leave blank to indicate no migration is necessary.
     * Set to `IMPOSSIBLE` to indicate downgrades are prohibited
     */
    down?: ((opts: MigrationOpts) => Promise<void>) | typeof IMPOSSIBLE
    /**
     * Additional migrations, such as fast-forward migrations, or migrations from other flavors.
     */
    other?: Record<
      string,
      {
        up?: (opts: MigrationOpts) => Promise<void>
        down?: (opts: MigrationOpts) => Promise<void>
      }
    >
  }
}

/**
 * Represents a single version of the service, including its release notes,
 * migration scripts, and backwards-compatibility declarations.
 *
 * @typeParam Version - The string literal exver version number
 */
export interface VersionInfo<Version extends string> {
  /** Brand to preserve type parameter distinction across structural checks */
  readonly _version: Version
  readonly options: VersionOptions<Version> & {
    satisfies: string[]
    satisfiesPackages: Record<string, string[]>
  }
  /** Declares full compatibility with a version of this package. */
  satisfies<V extends string>(
    version: V & ValidateExVer<V>,
  ): VersionInfo<Version>
  /**
   * Promises the named package version's host, interface, volume, action, and
   * health-check IDs and behavior. Claims are direct, not transitive; this
   * package's own version does not count toward another package ID.
   */
  satisfies<V extends string>(
    packageId: string,
    version: V & ValidateExVer<V>,
  ): VersionInfo<Version>
}

class VersionInfoImpl<Version extends string> implements VersionInfo<Version> {
  readonly _version: Version = null as any as Version
  constructor(readonly options: VersionInfo<Version>['options']) {}
  satisfies(
    packageIdOrVersion: string,
    version?: string,
  ): VersionInfo<Version> {
    if (version === undefined) {
      return new VersionInfoImpl({
        ...this.options,
        satisfies: [...this.options.satisfies, packageIdOrVersion],
      })
    }
    const packages = this.options.satisfiesPackages
    const claimed = Object.prototype.hasOwnProperty.call(
      packages,
      packageIdOrVersion,
    )
      ? packages[packageIdOrVersion]
      : []
    return new VersionInfoImpl({
      ...this.options,
      satisfiesPackages: {
        ...packages,
        [packageIdOrVersion]: [...claimed, version],
      },
    })
  }
}

/** Defines service versions for a VersionGraph. */
export const VersionInfo = {
  of<
    Version extends string,
    const Other extends Record<string, Migration> = {},
  >(
    options: VersionOptions<Version> & {
      migrations: { other?: Other & ValidateOtherKeys<Other> }
    },
  ): VersionInfo<Version> {
    return new VersionInfoImpl<Version>({
      ...options,
      satisfies: [],
      satisfiesPackages: {},
    })
  },
}

function __type_tests() {
  const version: VersionInfo<'1.0.0:0'> = VersionInfo.of({
    version: '1.0.0:0',
    releaseNotes: '',
    migrations: {},
  })
    .satisfies('#other:1.0.0:0')
    .satisfies('#other:2.0.0:0')
    .satisfies('another-package', '3.0.0:0')
    .satisfies('another-package', '#other:4.0.0:1')
    // @ts-expect-error - invalid own-package ExVer
    .satisfies('#other:2.f.0:0')
    // @ts-expect-error - invalid cross-package ExVer
    .satisfies('another-package', '#other:2.f.0:0')
    // @ts-expect-error - ranges are not concrete compatibility versions
    .satisfies('another-package', '>=1.0.0:0')
    // @ts-expect-error - widened strings cannot be validated
    .satisfies('another-package', '1.0.0:0' as string)

  let a: VersionInfo<'1.0.0:0'> = version
  // @ts-expect-error
  let b: VersionInfo<'1.0.0:3'> = version

  // migrations.other keys accept the full range grammar (and concrete versions)
  VersionInfo.of({
    version: '1.0.0:0',
    releaseNotes: '',
    migrations: {
      other: {
        ['^29']: { up: async () => {} },
        ['>=28:0 && <30:0']: { up: async () => {} },
        ['^#knotsrdts:29.3']: { up: async () => {} },
        ['1.0.0:0']: { up: async () => {} },
      },
    },
  })
  VersionInfo.of({
    version: '1.0.0:0',
    releaseNotes: '',
    migrations: {
      other: {
        // @ts-expect-error - invalid version inside the range key
        ['^28 || 30.x:0']: { up: async () => {} },
      },
    },
  })

  VersionInfo.of({
    // @ts-expect-error
    version: 'test',
    releaseNotes: '',
    migrations: {},
  })
  VersionInfo.of({
    // @ts-expect-error
    version: 'test' as string,
    releaseNotes: '',
    migrations: {},
  })
}
