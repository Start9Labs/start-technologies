import { inject, Service } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { T } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import {
  catchError,
  combineLatest,
  from,
  map,
  merge,
  of,
  shareReplay,
  startWith,
  Subject,
  switchMap,
} from 'rxjs'

import { DomainHealthService } from 'src/app/routes/portal/components/interfaces/addresses/gateway/domain-health.service'
import { GatewayAddress } from 'src/app/routes/portal/components/interfaces/interface.service'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'

@Service()
export class SharingAddresses extends DomainHealthService {
  private readonly sharingApi = inject(ApiService)
  private readonly sharingPatch = inject<PatchDB<DataModel>>(PatchDB)

  readonly refresh = new Subject<void>()
  private readonly config$ = merge(
    this.sharingPatch.watch$('serverInfo', 'network'),
    this.refresh,
  ).pipe(
    switchMap(() =>
      from(this.sharingApi.rpc.server.bittorrent.config({})).pipe(
        catchError(() => of('error' as const)),
      ),
    ),
    startWith('loading' as const),
    shareReplay({ bufferSize: 1, refCount: true }),
  )
  readonly config = toSignal(this.config$, { requireSync: true })
  readonly preferences = combineLatest([
    this.sharingPatch.watch$('serverInfo', 'bittorrent'),
    this.config$,
  ]).pipe(map(([preferences]) => preferences))

  protected override sendAddressSelection(
    enabled: boolean,
    address: GatewayAddress,
  ): Promise<T.HostnameInfo> {
    return this.sharingApi.rpc.server.bittorrent.setAddressEnabled({
      address: address.hostnameInfo,
      enabled,
    })
  }

  protected override async sendGuaSelection(
    wan: boolean,
    address: GatewayAddress,
  ): Promise<void> {
    await this.sharingApi.rpc.server.bittorrent.setGuaWan({
      address: address.hostnameInfo,
      wan,
    })
  }
}
