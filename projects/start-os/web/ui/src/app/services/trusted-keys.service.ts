import { inject, Service } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { MarketplaceTrust, TrustContext } from '@start9labs/marketplace'
import { DialogService, TaskService } from '@start9labs/shared'
import { PatchDB } from 'patch-db-client'
import { DataModel } from './patch-db/data-model'

@Service()
export class TrustedKeysService extends MarketplaceTrust {
  private readonly dialogs = inject(DialogService)
  private readonly tasks = inject(TaskService)

  readonly keys = toSignal(
    inject<PatchDB<DataModel>>(PatchDB).watch$('serverInfo', 'trustedKeys'),
  )

  manage(context: TrustContext = 'marketplace'): void {
    this.tasks.run(async () => {
      const { TRUSTED_KEYS } =
        await import('src/app/components/trusted-keys/dialog')
      this.dialogs.openComponent(TRUSTED_KEYS, { data: context }).subscribe()
    }, 'Loading')
  }
}
