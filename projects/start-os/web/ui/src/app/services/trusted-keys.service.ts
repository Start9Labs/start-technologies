import { inject, Service } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { MarketplaceTrust } from '@start9labs/marketplace'
import { PatchDB } from 'patch-db-client'
import { DataModel } from './patch-db/data-model'

@Service()
export class TrustedKeysService extends MarketplaceTrust {
  readonly keys = toSignal(
    inject<PatchDB<DataModel>>(PatchDB).watch$('serverInfo', 'trustedKeys'),
  )
}
