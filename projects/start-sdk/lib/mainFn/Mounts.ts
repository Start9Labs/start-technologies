import * as T from '@start9labs/start-core/types'
import { IdMap, MountOptions } from '../util/SubContainer'

type MountArray = { mountpoint: string; options: MountOptions }[]

type SharedOptions = {
  /** The path within the resource to mount. Use `null` to mount the entire resource */
  subpath: string | null
  /** Where to mount the resource. e.g. /data */
  mountpoint: string
  /**
   * Whether to mount this as a file or directory
   *
   * defaults to "directory"
   * */
  type?: 'file' | 'directory' | 'infer'
  /**
   * Optional UID/GID remapping for the mount, written from the
   * perspective of this container — `fromId` is the id seen on the
   * filesystem and `toId` is the id mounted processes see. The base
   * LXC mapping is applied automatically; do not include it here.
   *
   * https://www.kernel.org/doc/html/latest/filesystems/idmappings.html
   */
  idmap?: {
    /** The (starting) id of the data on the filesystem (u) */
    fromId: number
    /** The (starting) id of the data in the mount point (k) */
    toId: number
    /**
     * Optional: the number of incremental ids to map (r)
     *
     * defaults to 1
     * */
    range?: number
  }[]
}

function normalizeIdmap(
  idmap: NonNullable<SharedOptions['idmap']> | undefined,
): { fromId: number; toId: number; range: number }[] {
  return (idmap ?? []).map(i => ({
    fromId: i.fromId,
    toId: i.toId,
    range: i.range ?? 1,
  }))
}

type VolumeOpts<Manifest extends T.SDKManifest> = {
  /** The ID of the volume to mount. Must be one of the volume IDs defined in the manifest */
  volumeId: Manifest['volumes'][number]
  /** Whether or not the resource should be readonly for this subcontainer */
  readonly: boolean
} & SharedOptions

type DependencyOpts<Manifest extends T.SDKManifest> = {
  /** The ID of the dependency */
  dependencyId: Manifest['id']
  /** The ID of the volume to mount. Must be one of the volume IDs defined in the manifest of the dependency */
  volumeId: Manifest['volumes'][number]
  /** Whether or not the resource should be readonly for this subcontainer */
  readonly: boolean
} & Omit<SharedOptions, 'type'>

/**
 * Immutable builder for declaring filesystem mounts into a subcontainer.
 *
 * Supports mounting volumes, static assets, and dependency volumes.
 * Each `mount*` method returns a new `Mounts` instance (immutable builder pattern).
 *
 * @typeParam Manifest - The service manifest type
 */
export class Mounts<Manifest extends T.SDKManifest> {
  private constructor(
    readonly volumes: VolumeOpts<Manifest>[],
    readonly assets: SharedOptions[],
    readonly dependencies: DependencyOpts<T.SDKManifest>[],
  ) {}

  /**
   * Create an empty Mounts builder with no mounts configured.
   * @returns A new Mounts instance ready for chaining mount declarations
   */
  static of<Manifest extends T.SDKManifest>() {
    return new Mounts<Manifest>([], [], [])
  }

  /**
   * Add a volume mount from the service's own volumes.
   * @param options - Volume ID, mountpoint, readonly flag, and optional subpath
   * @returns A new Mounts instance with this volume added
   */
  mountVolume(options: VolumeOpts<Manifest>) {
    return new Mounts<Manifest>(
      [...this.volumes, options],
      [...this.assets],
      [...this.dependencies],
    )
  }

  /**
   * Add a read-only mount of the service's packaged static assets.
   * @param options - Mountpoint and optional subpath within the assets directory
   * @returns A new Mounts instance with this asset mount added
   */
  mountAssets(options: SharedOptions) {
    return new Mounts<Manifest>(
      [...this.volumes],
      [...this.assets, options],
      [...this.dependencies],
    )
  }

  /**
   * Add a mount from a dependency package's volume. The mount is always a
   * directory — StartOS does not bind a dependency's file. To reach a single
   * file, mount the directory holding it.
   *
   * @param options - Dependency ID, volume ID, mountpoint, readonly flag, and optional subpath
   * @returns A new Mounts instance with this dependency mount added
   */
  mountDependency<DependencyManifest extends T.SDKManifest>(
    options: DependencyOpts<DependencyManifest>,
  ) {
    return new Mounts<Manifest>(
      [...this.volumes],
      [...this.assets],
      [...this.dependencies, options],
    )
  }

  /**
   * Compile all declared mounts into the low-level mount array consumed by the subcontainer runtime.
   * @throws If any two mounts share the same mountpoint
   * @returns An array of `{ mountpoint, options }` objects
   */
  build(): MountArray {
    const mountpoints = new Set()
    for (let mountpoint of this.volumes
      .map(v => v.mountpoint)
      .concat(this.assets.map(a => a.mountpoint))
      .concat(this.dependencies.map(d => d.mountpoint))) {
      if (mountpoints.has(mountpoint)) {
        throw new Error(
          `cannot mount more than once to mountpoint ${mountpoint}`,
        )
      }
      mountpoints.add(mountpoint)
    }
    return ([] as MountArray)
      .concat(
        this.volumes.map(v => ({
          mountpoint: v.mountpoint,
          options: {
            type: 'volume',
            volumeId: v.volumeId,
            subpath: v.subpath,
            readonly: v.readonly,
            filetype: v.type ?? 'directory',
            idmap: normalizeIdmap(v.idmap),
          },
        })),
      )
      .concat(
        this.assets.map(a => ({
          mountpoint: a.mountpoint,
          options: {
            type: 'assets',
            subpath: a.subpath,
            filetype: a.type ?? 'directory',
            idmap: normalizeIdmap(a.idmap),
          },
        })),
      )
      .concat(
        this.dependencies.map(d => ({
          mountpoint: d.mountpoint,
          options: {
            type: 'pointer',
            packageId: d.dependencyId,
            volumeId: d.volumeId,
            subpath: d.subpath,
            readonly: d.readonly,
            idmap: normalizeIdmap(d.idmap),
          },
        })),
      )
  }
}
