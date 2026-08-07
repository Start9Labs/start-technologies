import { T } from '..'
import { ImageId, ImageSource } from '../types'

export type SDKManifest = {
  /**
   * The package identifier used by StartOS. This must be unique amongst all other known packages.
   * @example nextcloud
   * */
  readonly id: string
  /**
   * The human readable name of your service
   * @example Nextcloud
   * */
  readonly title: string
  /**
   * The name of the software license for this project. The license itself should be included in the root of the project directory.
   * @example MIT
   */
  readonly license: string
  /**
   * URL of the StartOS package repository
   * @example `https://github.com/Start9Labs/nextcloud-startos`
   */
  readonly packageRepo: string
  /**
   * URL of the upstream service repository
   * @example `https://github.com/nextcloud/docker`
   */
  readonly upstreamRepo: string
  /**
   * URL where users can learn more about the upstream service
   * @example `https://nextcloud.com`
   */
  readonly marketingUrl: string
  /**
   * (optional) URL where users can donate to the upstream project
   * @example `https://nextcloud.com/contribute/`
   */
  readonly donationUrl: string | null
  readonly description: {
    /** Short description to display on the marketplace list page. Max length 80 chars. */
    readonly short: T.LocaleString
    /** Long description to display on the marketplace details page for this service. Max length 500 chars. */
    readonly long: T.LocaleString
  }
  /**
   * override the StartOS version this package was made for
   * defaults to the version the SDK was built for
   */
  osVersion?: string
  /**
   * @description A mapping of OS images needed to run the container processes. Each image ID is a unique key.
   * @example
   * Using dockerTag...
   *
   * ```
    images: {
      main: {
        source: {
          dockerTag: 'start9/hello-world',
        },
      },
    },
   * ```
   * @example
   * Using dockerBuild...
   *
   * ```
    images: {
      main: {
        source: {
          dockerBuild: {
            dockerFile: '../Dockerfile',
            workdir: '.',
          },
        },
      },
    },
   * ```
   */
  readonly images: Record<ImageId, SDKImageInputSpec>
  /**
   * @description A list of data volumes that will mount to the container. Must contain at least one volume.
   * @example ['main']
   */
  readonly volumes: string[]

  /**
   * @description A mapping of service dependencies to be displayed to users when viewing the Marketplace
   * @property {string} description - An explanation of why this service is a dependency.
   * @property {boolean} optional - Whether or not this dependency is required or contingent on user configuration.
   * @property {string} s9pk - A path or url to an s9pk of the dependency to extract metadata at build time
   * @example
   * ```
    dependencies: {
      'hello-world': {
        description: 'A moon needs a world',
        optional: false,
        s9pk: '',
      },
    },
   * ```
   */
  readonly dependencies: Record<string, ManifestDependency>
  /**
   * @description (optional) A set of hardware requirements for this service. A machine that does
   *   not meet them is not offered this service at all — so tightening a requirement on an
   *   already-published package cuts hosts below it off from further updates.
   * @property {object[]} device - Device filters; at least one device of the given `class` must
   *    match each filter. `product` and `vendor` are regular expressions (`null` matches any), and
   *    `description` is what the user is shown about the hardware required.
   * @property {number} ram - Minimum RAM, **in bytes**, compared against the host's total RAM.
   *    No unit conversion is applied, so write it as a power-of-two expression: a bare `8192`
   *    declares 8 KiB, which every machine satisfies, and gates nothing.
   * @example
   * ```
    hardwareRequirements: {
      device: [
        { class: 'display', product: null, vendor: null, driver: 'nvidia', description: 'An NVIDIA GPU' },
      ],
      ram: 8 * 1024 ** 3, // 8 GiB
    },
   * ```
   */
  readonly hardwareRequirements?: {
    readonly device?: T.DeviceFilter[]
    readonly ram?: number | null
  }

  /**
   * @description Enable access to hardware acceleration devices (such as /dev/dri, or /dev/nvidia*)
   */
  readonly hardwareAcceleration?: boolean

  /**
   * @description Mount /dev/fuse so this service can use fuse-overlayfs — the
   * rootless storage driver used to host a nested OCI runtime (e.g. rootless
   * Podman or Docker) inside the service's LXC.
   */
  readonly userspaceFilesystems?: boolean

  /**
   * @description Mount /dev/net/tun so this service can create kernel tunnel
   * interfaces (VPN / WireGuard / tun-class workloads).
   */
  readonly virtualNetworking?: boolean

  /**
   * @description Enable OS plugins
   */
  readonly plugins?: T.PluginId[]
}

// this is hacky but idk a more elegant way
type ArchOptions = {
  0: ['x86_64', 'aarch64', 'riscv64']
  1: ['aarch64', 'x86_64', 'riscv64']
  2: ['x86_64', 'riscv64', 'aarch64']
  3: ['aarch64', 'riscv64', 'x86_64']
  4: ['riscv64', 'x86_64', 'aarch64']
  5: ['riscv64', 'aarch64', 'x86_64']
  6: ['x86_64', 'aarch64']
  7: ['aarch64', 'x86_64']
  8: ['x86_64', 'riscv64']
  9: ['aarch64', 'riscv64']
  10: ['riscv64', 'aarch64']
  11: ['riscv64', 'x86_64']
  12: ['x86_64']
  13: ['aarch64']
  14: ['riscv64']
}
export type SDKImageInputSpec = {
  [A in keyof ArchOptions]: {
    source: Exclude<ImageSource, 'packed'>
    arch?: ArchOptions[A]
    emulateMissingAs?: ArchOptions[A][number] | null
    nvidiaContainer?: boolean
  }
}[keyof ArchOptions]

export type ManifestDependency = T.Manifest['dependencies'][string]
