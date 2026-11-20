import { inject, Service } from '@angular/core'
import { isNetworkError } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import { catchError, combineLatest, EMPTY, from, Observable, switchMap } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { ConnectionService } from 'src/app/services/connection.service'
import { DataModel } from 'src/app/services/patch-db/data-model'

@Service()
export class SshKeys extends Observable<T.SshKeyResponse[]> {
  constructor() {
    const api = inject(ApiService)
    const patch = inject<PatchDB<DataModel>>(PatchDB)
    const connection = inject(ConnectionService)
    super(subscriber =>
      combineLatest([patch.watch$('sshPubkeys'), connection])
        .pipe(
          switchMap(([, connected]) =>
            connected
              ? from(api.rpc.ssh.list({})).pipe(
                  catchError(error => {
                    if (isNetworkError(error)) return EMPTY
                    throw error
                  }),
                )
              : EMPTY,
          ),
        )
        .subscribe(subscriber),
    )
  }
}
