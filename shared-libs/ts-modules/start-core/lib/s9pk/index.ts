import {
  DataUrl,
  DependencyMetadata,
  Manifest,
  MerkleArchiveCommitment,
  PackageId,
  SideloadParams,
} from '../osBindings'
import { ArrayBufferReader, MerkleArchive } from './merkleArchive'
import mime from 'mime'
import { DirectoryContents } from './merkleArchive/directoryContents'
import { FileContents } from './merkleArchive/fileContents'
import { readTrailer, S9pkDiagnostic } from './trailer'
export type { S9pkDiagnostic } from './trailer'

const magicAndVersion = new Uint8Array([59, 59, 2])
export const s9pkHeaderSize = magicAndVersion.length + MerkleArchive.headerSize

export function compare(a: Uint8Array, b: Uint8Array) {
  if (a.length !== b.length) return false
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false
  }
  return true
}

/**
 * Represents a parsed `.s9pk` package archive — the binary distribution format for StartOS services.
 *
 * An `S9pk` wraps a parsed {@link Manifest}, a {@link MerkleArchive} containing the package's
 * assets (icon, license, dependency metadata), and the total archive size in bytes.
 *
 * @example
 * ```ts
 * const s9pk = await S9pk.deserialize(file, null)
 * console.log(s9pk.manifest.id)    // e.g. "bitcoind"
 * console.log(s9pk.size)           // archive size in bytes
 * const icon = await s9pk.icon()   // base64 data URL
 * const license = await s9pk.license()
 * ```
 */
export class S9pk {
  private constructor(
    /** The parsed package manifest containing metadata, dependencies, and interface definitions. */
    readonly manifest: Manifest,
    /** The Merkle-verified archive containing the package's files. */
    readonly archive: MerkleArchive,
    /** The total size of the archive in bytes. */
    readonly size: number,
    /** Claimed canonical DER64 signing identities; the backend must verify their signatures. */
    readonly signers: readonly string[],
    /** Supplemental signature candidates for backend verification against the archive. */
    readonly signatures: Readonly<SideloadParams['signatures']>,
    /** Optional metadata issues; these never replace base archive validation. */
    readonly diagnostics: readonly S9pkDiagnostic[],
  ) {}
  /** Reads a complete S9PK header and returns its format version. */
  static async getVersion(source: Blob): Promise<number> {
    const header = new Uint8Array(await source.slice(0, 3).arrayBuffer())
    if (
      header.length !== 3 ||
      !compare(header.slice(0, 2), magicAndVersion.slice(0, 2))
    ) {
      throw new Error('Invalid Magic or Unexpected Version')
    }
    return header[2]
  }

  /**
   * Deserializes an `S9pk` from a `Blob` (e.g. a `File` from a browser file input).
   *
   * Validates the magic bytes and version header, then parses the Merkle archive structure.
   * Signature bytes are not verified. Sideload callers forward the candidates and pin matched keys.
   * An external commitment additionally pins the archive's Merkle root and size bound.
   *
   * @param source - The raw `.s9pk` file as a `Blob`
   * @param commitment - An optional Merkle commitment to verify the archive against, or `null` to skip external commitment pinning
   * @returns A fully parsed `S9pk` instance
   * @throws If the magic bytes are invalid or the archive fails verification
   */
  static async deserialize(
    source: Blob,
    commitment: MerkleArchiveCommitment | null,
  ): Promise<S9pk> {
    if ((await S9pk.getVersion(source)) !== magicAndVersion[2])
      throw new Error('Invalid Magic or Unexpected Version')
    const header = new ArrayBufferReader(
      await source.slice(0, s9pkHeaderSize).arrayBuffer(),
    )
    header.next(magicAndVersion.length)

    const archive = await MerkleArchive.deserialize(
      source,
      's9pk',
      header,
      commitment,
    )

    const manifest = JSON.parse(
      new TextDecoder().decode(
        await archive.contents
          .getPath(['manifest.json'])
          ?.verifiedFileContents(),
      ),
    )

    const trailer = await readTrailer(source, archive)
    return new S9pk(
      manifest,
      archive,
      source.size,
      trailer.signers,
      trailer.signatures,
      trailer.diagnostics,
    )
  }
  /**
   * Extracts the package icon from the archive and returns it as a base64-encoded data URL.
   *
   * Looks for a file named `icon.*` with an image MIME type (e.g. `icon.png`, `icon.svg`).
   *
   * @returns A data URL string like `"data:image/png;base64,..."` suitable for use in `<img src>`.
   * @throws If no icon file is found in the archive
   */
  async icon(): Promise<DataUrl> {
    const iconName = Object.keys(this.archive.contents.contents).find(
      name =>
        name.startsWith('icon.') && mime.getType(name)?.startsWith('image/'),
    )
    if (!iconName) {
      throw new Error('no icon found in archive')
    }
    return (
      `data:${mime.getType(iconName)};base64,` +
      Buffer.from(
        await this.archive.contents.getPath([iconName])!.verifiedFileContents(),
      ).toString('base64')
    )
  }

  /**
   * Returns the metadata (e.g. `{ title }`) for a specific dependency by its package ID.
   *
   * @param id - The dependency's package identifier (e.g. `"bitcoind"`)
   * @returns The dependency metadata object, or `null` if the dependency is not present in the archive
   */
  async dependencyMetadataFor(id: PackageId) {
    const entry = this.archive.contents.getPath([
      'dependencies',
      id,
      'metadata.json',
    ])
    if (!entry) return null
    return JSON.parse(
      new TextDecoder().decode(await entry.verifiedFileContents()),
    ) as { title: string }
  }

  /**
   * Returns the icon for a specific dependency as a base64 data URL.
   *
   * @param id - The dependency's package identifier
   * @returns A data URL string, or `null` if the dependency or its icon is not present
   */
  async dependencyIconFor(id: PackageId) {
    const dir = this.archive.contents.getPath(['dependencies', id])
    if (!dir || !(dir.contents instanceof DirectoryContents)) return null
    const iconName = Object.keys(dir.contents.contents).find(
      name =>
        name.startsWith('icon.') && mime.getType(name)?.startsWith('image/'),
    )
    if (!iconName) return null
    return (
      `data:${mime.getType(iconName)};base64,` +
      Buffer.from(
        await dir.contents.getPath([iconName])!.verifiedFileContents(),
      ).toString('base64')
    )
  }

  /**
   * Returns a merged record of all dependency metadata (title, icon, description, optional flag)
   * for every dependency declared in the manifest.
   *
   * @returns A record keyed by package ID, each containing `{ title, icon, description, optional }`
   */
  async dependencyMetadata() {
    return Object.fromEntries(
      await Promise.all(
        Object.entries(this.manifest.dependencies)
          .filter(([_, info]) => !!info)
          .map(async ([id, info]) => [
            id,
            {
              ...(await this.dependencyMetadataFor(id)),
              icon: await this.dependencyIconFor(id),
              description: info!.description,
              optional: info!.optional,
            },
          ]),
      ),
    )
  }

  /**
   * Reads and returns the `LICENSE.md` file from the archive as a UTF-8 string.
   *
   * @returns The full license text
   * @throws If `LICENSE.md` is not found in the archive
   */
  async license(): Promise<string> {
    const file = this.archive.contents.getPath(['LICENSE.md'])
    if (!file || !(file.contents instanceof FileContents))
      throw new Error('license.md not found in archive')
    return new TextDecoder().decode(await file.verifiedFileContents())
  }

  /**
   * Reads and returns the `instructions.md` file from the archive as a UTF-8 string.
   *
   * `instructions.md` is a required, user-facing markdown document at the root of every
   * package — see the StartOS packaging book for guidance on what belongs in it.
   *
   * @returns The full instructions markdown, or `null` if the s9pk predates the
   *   instructions-required convention and does not contain the file
   */
  async instructions(): Promise<string | null> {
    const file = this.archive.contents.getPath(['instructions.md'])
    if (!file || !(file.contents instanceof FileContents)) return null
    return new TextDecoder().decode(await file.verifiedFileContents())
  }
}
