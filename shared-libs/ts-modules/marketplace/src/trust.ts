import { Pipe, Signal } from '@angular/core'
import { auth, RPC, T } from '@start9labs/start-core'
import { MarketplacePkgBase } from './types'

export type TrustContext = RPC.RpcParamType<
  RPC.StartOS,
  'server.trusted-keys.add'
>['context']

export abstract class MarketplaceTrust {
  abstract readonly keys: Signal<Record<string, T.TrustedKeyring> | undefined>
}

export function packageSigners(
  pkg: Pick<MarketplacePkgBase, 's9pks' | 'signers'>,
): string[] {
  return pkg.signers ?? Object.keys(pkg.s9pks[0]?.[1].signatures ?? {})
}

export function matchingTrustedKeys(
  signers: string[],
  groups: Record<string, T.TrustedKeyring>,
  context: TrustContext,
) {
  const signed = new Set(signers.map(auth.normalizeDer64))
  return Object.entries(groups).flatMap(([id, group]) => {
    const publicKeys = group.publicKeys.filter(key =>
      signed.has(auth.normalizeDer64(key)),
    )
    return group[context] && publicKeys.length
      ? [{ id, name: group.name, publicKeys }]
      : []
  })
}

@Pipe({ name: 'trustedPackageSigners' })
export class TrustedPackageSignersPipe {
  transform(
    pkg: Pick<MarketplacePkgBase, 's9pks' | 'signers'>,
    groups: Record<string, T.TrustedKeyring>,
  ) {
    return matchingTrustedKeys(packageSigners(pkg), groups, 'marketplace')
  }
}
