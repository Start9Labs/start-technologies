import { matchingTrustedKeys } from '@start9labs/marketplace'
import { T } from '@start9labs/start-core'

export function trustedOsUpdateSigners(
  signers: string[],
  trusted: Record<string, T.TrustedKeyring>,
): string[] {
  return matchingTrustedKeys(signers, trusted, 'os').flatMap(
    group => group.publicKeys,
  )
}
