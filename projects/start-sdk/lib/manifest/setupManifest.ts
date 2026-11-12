import { Version as ExverVersion } from '@start9labs/start-core/exver'
import * as T from '@start9labs/start-core/types'
import { ImageConfig, ImageId, VolumeId } from '@start9labs/start-core/types'
import {
  SDKManifest,
  SDKImageInputSpec,
} from '@start9labs/start-core/types/ManifestTypes'
import { OSVersion } from '../StartSdk'
import { Dependencies } from '../dependencies'
import { VersionGraph } from '../version/VersionGraph'
import sdkPackage from '@start9labs/start-sdk/package.json'

/** Rejects unknown fields that generic excess-property checks would otherwise accept. */
type NoExtraKeys<Manifest> = {
  [K in keyof Manifest]: K extends keyof SDKManifest
    ? Manifest[K]
    : 'Unknown manifest field — not declared in SDKManifest'
}

/**
 * @description Use this function to define critical information about your package
 *
 * @param manifest Static properties of the package
 */
export function setupManifest<
  Id extends string,
  VolumesTypes extends VolumeId,
  Manifest extends {
    id: Id
    volumes: VolumesTypes[]
  } & SDKManifest,
>(manifest: Manifest & SDKManifest & NoExtraKeys<Manifest>): Manifest {
  return manifest
}

/**
 * Build the final publishable manifest by combining the SDK manifest definition
 * with version graph metadata, OS version, SDK version, and computed fields
 * (migration ranges, hardware requirements, etc.).
 *
 * @param versions - The service's VersionGraph, used to extract the current version, release notes, and migration ranges
 * @param manifest - The SDK manifest definition (from `setupManifest`)
 * @returns A fully resolved Manifest ready for packaging
 */
export function buildManifest<
  Id extends string,
  Version extends string,
  DependencyIds extends string,
  VolumesTypes extends VolumeId,
  ImagesTypes extends ImageId,
  Manifest extends {
    id: Id
    images: Record<ImagesTypes, SDKImageInputSpec>
    volumes: VolumesTypes[]
  },
>(
  versions: VersionGraph<Version>,
  manifest: SDKManifest & Manifest,
  dependencies: Dependencies<DependencyIds>,
): Manifest & T.Manifest {
  const claimedPackages = Object.keys(
    versions.current.options.satisfiesPackages,
  )
  if (claimedPackages.includes(manifest.id)) {
    throw new Error(
      `Use .satisfies(version) for ${manifest.id}'s same-package aliases`,
    )
  }
  const images = Object.entries(manifest.images).reduce(
    (images, [k, v]) => {
      v.arch = v.arch ?? ['aarch64', 'x86_64', 'riscv64']
      v.emulateMissing = v.emulateMissing ?? true
      v.nvidiaContainer = !!v.nvidiaContainer
      images[k] = v as ImageConfig
      return images
    },
    {} as { [k: string]: ImageConfig },
  )
  let osVersion = manifest.osVersion ?? OSVersion
  if (
    claimedPackages.length &&
    ExverVersion.parse(osVersion).compare(ExverVersion.parse('0.4.3')) ===
      'less'
  ) {
    osVersion = '0.4.3'
  }
  return {
    ...manifest,
    dependencies: dependencies.manifestDependencies(),
    gitHash: null,
    osVersion,
    sdkVersion: sdkPackage.version,
    version: versions.current.options.version,
    releaseNotes: versions.current.options.releaseNotes,
    satisfies: versions.current.options.satisfies || [],
    satisfiesPackages: versions.current.options.satisfiesPackages,
    canMigrateTo: versions.canMigrateTo().toString(),
    canMigrateFrom: versions.canMigrateFrom().toString(),
    images,
    hardwareRequirements: {
      device: (manifest.hardwareRequirements?.device ?? []).map(d => ({
        product: null,
        vendor: null,
        capabilities: null,
        driver: null,
        ...d,
      })),
      ram: manifest.hardwareRequirements?.ram || null,
      arch: Object.values(images).reduce(
        (arch, inputSpec) => {
          if (inputSpec.emulateMissing) {
            return arch
          }
          if (arch === null) {
            return inputSpec.arch
          }
          return arch.filter(a => inputSpec.arch.includes(a))
        },
        null as string[] | null,
      ),
    },
    hardwareAcceleration: manifest.hardwareAcceleration ?? false,
    userspaceFilesystems: manifest.userspaceFilesystems ?? false,
    virtualNetworking: manifest.virtualNetworking ?? false,
    hardwareVirtualization: manifest.hardwareVirtualization ?? false,
    plugins: manifest.plugins ?? [],
  }
}
