import { inject, Injectable } from '@angular/core'
import { Router } from '@angular/router'
import { ErrorService, StartOSDiskInfo } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiDialogOptions, TuiDialogService } from '@taiga-ui/core'
import { TuiNotificationMiddleService } from '@taiga-ui/kit'
import {
  catchError,
  EMPTY,
  exhaustMap,
  map,
  Observable,
  of,
  switchMap,
  take,
} from 'rxjs'
import {
  PROMPT,
  PromptOptions,
} from 'src/app/routes/portal/modals/prompt.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { RECOVER } from '../modals/recover.component'
import { SERVERS } from '../modals/servers.component'
import { TARGET, TARGET_RESTORE } from '../modals/target.component'
import { RecoverData } from '../types/recover-data'

@Injectable({
  providedIn: 'root',
})
export class BackupsRestoreService {
  private readonly errorService = inject(ErrorService)
  private readonly dialogs = inject(TuiDialogService)
  private readonly router = inject(Router)
  private readonly api = inject(ApiService)
  private readonly loader = inject(TuiNotificationMiddleService)

  readonly handle = () => {
    this.dialogs
      .open<T.BackupTarget & { id: string }>(TARGET, TARGET_RESTORE)
      .pipe(
        switchMap(target =>
          this.dialogs
            .open<StartOSDiskInfo & { id: string }>(SERVERS, {
              label: 'Select server',
              data: { servers: Object.values(target.startOs) },
            })
            .pipe(
              switchMap(({ id }) =>
                this.dialogs.open<string>(PROMPT, PROMPT_OPTIONS).pipe(
                  exhaustMap(password =>
                    this.getRecoverData(target.id, id, password),
                  ),
                  take(1),
                  switchMap(data =>
                    this.dialogs.open(RECOVER, {
                      label: 'Select Services to Restore',
                      data,
                    }),
                  ),
                ),
              ),
            ),
        ),
      )
      .subscribe(() => {
        this.router.navigate(['services'])
      })
  }

  private getRecoverData(
    targetId: string,
    serverId: string,
    password: string,
  ): Observable<RecoverData> {
    return of(password).pipe(
      switchMap(() => {
        const loader = this.loader.open('Decrypting drive').subscribe()

        return this.api.rpc.backup.target
          .info({ targetId, password })
          .finally(() => loader.unsubscribe())
      }),
      catchError(e => {
        this.errorService.handleError(e)

        return EMPTY
      }),
      map(backupInfo => ({ targetId, password, backupInfo, serverId })),
    )
  }
}

const PROMPT_OPTIONS: Partial<TuiDialogOptions<PromptOptions>> = {
  label: 'Password Required',
  data: {
    message: `Enter the master password that was used to encrypt this backup. On the next screen, you will select the individual services you want to restore.`,
    label: 'Master Password',
    placeholder: 'Enter master password',
    useMask: true,
  },
}
