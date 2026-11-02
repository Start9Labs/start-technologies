import { Pipe, Signal } from '@angular/core'
import { auth, RPC, T } from '@start9labs/start-core'
import { MarketplacePkgBase } from './types'
import { signingKeyIdentity } from './key-identity'

export type TrustContext = RPC.RpcParamType<
  RPC.StartOS,
  'server.trusted-keys.add'
>['context']

export abstract class MarketplaceTrust {
  abstract readonly keys: Signal<Record<string, T.TrustedKey> | undefined>
}

export function packageSigners(
  pkg: Pick<MarketplacePkgBase, 's9pks' | 'signers'>,
): string[] {
  return pkg.signers ?? Object.keys(pkg.s9pks[0]?.[1].signatures ?? {})
}

export function matchingTrustedKeys(
  signers: string[],
  keys: Record<string, T.TrustedKey>,
  context: TrustContext,
) {
  const signed = new Set(signers.map(auth.normalizeDer64))
  return Object.entries(keys).flatMap(([id, key]) =>
    key[context] && signed.has(auth.normalizeDer64(key.publicKey))
      ? [{ id, name: key.name, publicKey: key.publicKey }]
      : [],
  )
}

@Pipe({ name: 'packageSignerIdentities' })
export class PackageSignerIdentitiesPipe {
  transform(
    pkg: Pick<MarketplacePkgBase, 's9pks' | 'signers'>,
    keys: Record<string, T.TrustedKey>,
  ) {
    return [...new Set(packageSigners(pkg).map(auth.normalizeDer64))].map(
      id => {
        const key = Object.values(keys).find(
          key => auth.normalizeDer64(key.publicKey) === id,
        )
        return { id, ...signingKeyIdentity(id, key) }
      },
    )
  }
}
