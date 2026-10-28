import { T, auth } from '@start9labs/start-core'

export function trustedOsUpdateSigners(
  signers: string[],
  trusted: Record<string, T.TrustedOsUpdateSigner>,
): string[] {
  const keys = new Set(Object.keys(trusted).map(auth.normalizeDer64))
  return signers.filter(signer => keys.has(auth.normalizeDer64(signer)))
}
