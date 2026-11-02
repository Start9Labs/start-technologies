import { Pipe, Signal } from '@angular/core'
import { auth, T } from '@start9labs/start-core'
import { MarketplacePkgBase } from './types'
import { signingKeyIdentity } from './key-identity'

export type TrustContext = 'marketplace' | 'os'

export abstract class MarketplaceTrust {
  abstract readonly keys: Signal<Record<string, T.TrustedKey> | undefined>
}

export function packageSigners(
  pkg: Pick<MarketplacePkgBase, 's9pks' | 'signers'>,
): string[] {
  return pkg.signers ?? Object.keys(pkg.s9pks[0]?.[1].signatures ?? {})
}

function matchingSigningKeys(
  signers: string[],
  keys: Record<string, T.TrustedKey>,
) {
  const signed = new Set(signers.map(auth.normalizeDer64))
  return Object.entries(keys).filter(([, key]) =>
    signed.has(auth.normalizeDer64(key.publicKey)),
  )
}

export function matchingTrustedKeys(
  signers: string[],
  keys: Record<string, T.TrustedKey>,
  context: TrustContext,
) {
  return matchingSigningKeys(signers, keys).flatMap(([id, key]) =>
    key[context] ? [{ id, name: key.name, publicKey: key.publicKey }] : [],
  )
}

@Pipe({ name: 'packageSignerIdentities' })
export class PackageSignerIdentitiesPipe {
  transform(
    pkg: Pick<MarketplacePkgBase, 's9pks' | 'signers'>,
    keys: Record<string, T.TrustedKey>,
  ) {
    const known = matchingSigningKeys(packageSigners(pkg), keys)
    const enabled = known.filter(([, key]) => key.marketplace)
    return (enabled.length ? enabled : known).map(([id, key]) => ({
      id,
      ...signingKeyIdentity(key.publicKey, key),
    }))
  }
}
