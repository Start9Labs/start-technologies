import { T } from '@start9labs/start-core'

const normalizeKey = (key: string) => key.replace(/\s/g, '')

export function trustedOsUpdateSigners(
  signers: string[],
  trusted: Record<string, T.TrustedOsUpdateSigner>,
): string[] {
  const keys = new Set(Object.keys(trusted).map(normalizeKey))
  return signers.filter(signer => keys.has(normalizeKey(signer)))
}
