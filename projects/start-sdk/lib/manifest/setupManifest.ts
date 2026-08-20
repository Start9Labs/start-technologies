import * as T from '@start9labs/start-core/types'
import { ImageConfig, ImageId, VolumeId } from '@start9labs/start-core/types'
import {
  SDKManifest,
  SDKImageInputSpec,
} from '@start9labs/start-core/types/ManifestTypes'
import { OSVersion } from '../StartSdk'
import { VersionGraph } from '../version/VersionGraph'
import { version as sdkVersion } from '../../package.json'

/**
 * Retypes every key the SDK does not define, so declaring one is a compile
 * error at that key rather than a field that silently does nothing. The
 * replacement is a string literal rather than `never` purely so the error names
 * what went wrong.
 *
 * TypeScript applies its excess-property check only when a fresh object literal
 * meets a non-generic target, and `Manifest` is inferred from the argument — so
 * without this an unknown field joins the inferred type, typechecks, and is
 * then dropped on the way to the built manifest.
 */
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
  Dependencies extends Record<string, unknown>,
  VolumesTypes extends VolumeId,
  ImagesTypes extends ImageId,
  Manifest extends {
    dependencies: Dependencies
    id: Id
    images: Record<ImagesTypes, SDKImageInputSpec>
    volumes: VolumesTypes[]
  },
>(
  versions: VersionGraph<Version>,
  manifest: SDKManifest & Manifest,
): Manifest & T.Manifest {
  const images = Object.entries(manifest.images).reduce(
    (images, [k, v]) => {
      v.arch = v.arch ?? ['aarch64', 'x86_64', 'riscv64']
      if (v.emulateMissingAs === undefined)
        v.emulateMissingAs = (v.arch as string[]).includes('x86_64')
          ? 'x86_64'
          : (v.arch[0] ?? null)
      v.nvidiaContainer = !!v.nvidiaContainer
      images[k] = v as ImageConfig
      return images
    },
    {} as { [k: string]: ImageConfig },
  )
  return {
    ...manifest,
    gitHash: null,
    osVersion: manifest.osVersion ?? OSVersion,
    sdkVersion,
    version: versions.current.options.version,
    releaseNotes: versions.current.options.releaseNotes,
    satisfies: versions.current.options.satisfies || [],
    canMigrateTo: versions.canMigrateTo().toString(),
    canMigrateFrom: versions.canMigrateFrom().toString(),
    images,
    hardwareRequirements: {
      device: manifest.hardwareRequirements?.device || [],
      ram: manifest.hardwareRequirements?.ram || null,
      arch: Object.values(images).reduce(
        (arch, inputSpec) => {
          if (inputSpec.emulateMissingAs) {
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
    plugins: manifest.plugins ?? [],
  }
}
