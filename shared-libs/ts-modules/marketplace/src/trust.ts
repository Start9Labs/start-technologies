import { Pipe, Signal } from '@angular/core'
import { auth, T } from '@start9labs/start-core'
import { MarketplacePkgBase } from './types'
import { signingKeyIdentity } from './key-identity'

export type TrustContext = T.TrustContext

export abstract class MarketplaceTrust {
  abstract readonly keys: Signal<Record<string, T.TrustedKey> | undefined>
  abstract viewSigner(signer: Signer): void
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

export type SignerStatus = 'trusted' | 'untrusted' | 'unknown'

export type Signer = ReturnType<typeof signingKeyIdentity> & {
  id: string
  status: SignerStatus
}

const STATUS_ORDER: SignerStatus[] = ['trusted', 'untrusted', 'unknown']

/** Each key with its trust on this server, for packages by default. */
@Pipe({ name: 'signerStatuses' })
export class SignerStatusesPipe {
  transform(
    signers: readonly string[],
    keys: Record<string, T.TrustedKey>,
    context: TrustContext = 'marketplace',
  ): Signer[] {
    const known = new Map(
      Object.values(keys).map(key => [auth.normalizeDer64(key.publicKey), key]),
    )

    return [...new Set(signers.map(auth.normalizeDer64))]
      .map(id => {
        const key = known.get(id)
        const status: SignerStatus = !key
          ? 'unknown'
          : key[context]
            ? 'trusted'
            : 'untrusted'

        return { id, status, ...signingKeyIdentity(id, key) }
      })
      .sort(
        (a, b) =>
          STATUS_ORDER.indexOf(a.status) - STATUS_ORDER.indexOf(b.status),
      )
  }
}
