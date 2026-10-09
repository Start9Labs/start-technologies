import { inject, Injectable } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { DialogService, TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import { defer, filter, map, Observable, switchMap, take } from 'rxjs'
import { POWER } from 'src/app/routes/portal/components/header/power.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { OSService } from 'src/app/services/os.service'
import { DataModel } from 'src/app/services/patch-db/data-model'

@Injectable({ providedIn: 'root' })
export class PowerService {
  private readonly api = inject(ApiService)
  private readonly dialog = inject(DialogService)
  private readonly tasks = inject(TaskService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  readonly backingUp = toSignal(inject(OSService).backingUp$, {
    initialValue: false,
  })

  /**
   * Requests backup-safe power unless the user chooses immediate interruption.
   * Emits whether server-owned power is deferred after a successful request.
   * Dismissal and failed requests emit nothing.
   */
  power(action: T.PowerAction): Observable<boolean> {
    if (!this.backingUp()) return this.run(action, false)

    return this.dialog
      .openComponent<boolean>(POWER, {
        label: action === 'restart' ? 'Restart' : 'Warning',
        size: 's',
        data: action,
      })
      .pipe(switchMap(force => this.run(action, force)))
  }

  cancel() {
    this.tasks.run(async () => await this.api.cancelDeferredPower({}))
  }

  private run(action: T.PowerAction, force: boolean): Observable<boolean> {
    return defer(() =>
      this.tasks.run(
        async () =>
          action === 'restart'
            ? await this.api.restartServer({ force })
            : await this.api.shutdownServer({ force }),
        !force && this.backingUp()
          ? 'Wait for backup to complete'
          : `Beginning ${action}`,
      ),
    ).pipe(
      filter(Boolean),
      switchMap(() =>
        this.patch
          .watch$('serverInfo', 'statusInfo', 'deferredPowerAction')
          .pipe(take(1), map(Boolean)),
      ),
    )
  }
}
