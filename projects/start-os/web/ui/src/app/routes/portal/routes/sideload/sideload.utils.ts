import { MarketplacePkgBase } from '@start9labs/marketplace'
import { i18nKey } from '@start9labs/shared'
import { ExtendedVersion, S9pk } from '@start9labs/start-core'

export async function validateS9pk(
  file: File,
): Promise<MarketplacePkgSideload | i18nKey> {
  try {
    const version = await S9pk.getVersion(file)
    if (version === 1) {
      return 'Version 1 s9pk detected. This package format is deprecated. You can sideload a V1 s9pk via start-cli if necessary.'
    }
    if (version !== 2) return 'Invalid package file'

    const s9pk = await S9pk.deserialize(file, null)
    return {
      ...s9pk.manifest,
      dependencyMetadata: await s9pk.dependencyMetadata(),
      icon: await s9pk.icon(),
      sourceVersion: s9pk.manifest.canMigrateFrom,
      flavor: ExtendedVersion.parse(s9pk.manifest.version).flavor,
      fullLicense: await s9pk.license(),
      s9pks: [],
      signers: [...s9pk.signers],
      signatureDiagnostics: s9pk.diagnostics,
    }
  } catch (error) {
    console.error(error)
    return 'Invalid package file'
  }
}

export type MarketplacePkgSideload = MarketplacePkgBase & {
  fullLicense: string
  signatureDiagnostics: S9pk['diagnostics']
}
