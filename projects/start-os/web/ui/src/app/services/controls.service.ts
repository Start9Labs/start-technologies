import { inject, Injectable } from '@angular/core'
import {
  DialogService,
  i18nKey,
  i18nPipe,
  i18nService,
  TaskService,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import {
  defaultIfEmpty,
  defer,
  filter,
  firstValueFrom,
  Observable,
  of,
} from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { getAllPackages } from 'src/app/utils/get-package-data'
import { hasCurrentDeps } from 'src/app/utils/has-deps'
import { confirmForceStop, forceStopAt$ } from './force-stop'
import { TimeService } from './time.service'

@Injectable({
  providedIn: 'root',
})
export class ControlsService {
  private readonly dialog = inject(DialogService)
  private readonly tasks = inject(TaskService)
  private readonly api = inject(ApiService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly i18n = inject(i18nPipe)
  private readonly i18nService = inject(i18nService)
  private readonly time = inject(TimeService)

  forceStopAt$(status$: Observable<T.StatusInfo>) {
    return forceStopAt$(status$, this.time.now$)
  }

  forceStop(params: T.ForceStopParams) {
    return confirmForceStop(
      params,
      () =>
        this.dialog.openConfirm({
          label: 'Warning',
          size: 's',
          data: {
            content:
              'Force stopping may cause data loss or leave the service in a bad state. Are you sure you want to force stop?',
            yes: 'Force stop',
            no: 'Cancel',
          },
        }),
      request =>
        this.tasks.run(
          () => this.api.forceStopPackage(request),
          'Force stopping',
        ),
    )
  }

  async start({ title, id }: T.Manifest, unmet: boolean) {
    const deps =
      `${title} ${this.i18n.transform('has unmet dependencies. It will not work as expected.')}` as i18nKey

    if (!unmet || (await this.alert(deps))) {
      this.tasks.run(
        async () => await this.api.startPackage({ id }),
        'Starting',
      )
    }
  }

  async stop({ id, title }: T.Manifest) {
    let content = ''

    if (hasCurrentDeps(id, await getAllPackages(this.patch))) {
      content = `${this.i18n.transform('Services that depend on')} ${title} ${this.i18n.transform('will no longer work properly and may crash.')}`
    }

    defer(() =>
      content
        ? this.dialog
            .openConfirm({
              label: 'Warning',
              size: 's',
              data: {
                content: content as i18nKey,
                yes: 'Stop',
                no: 'Cancel',
              },
            })
            .pipe(filter(Boolean))
        : of(null),
    ).subscribe(() =>
      this.tasks.run(
        async () => await this.api.stopPackage({ id }),
        'Stopping',
      ),
    )
  }

  async restart(id: string) {
    this.tasks.run(
      async () => await this.api.restartPackage({ id }),
      'Restarting',
    )
  }

  private alert(content: T.LocaleString): Promise<boolean> {
    return firstValueFrom(
      this.dialog
        .openConfirm({
          label: 'Warning',
          size: 's',
          data: {
            content: this.i18nService.localize(content),
            yes: 'Continue',
            no: 'Cancel',
          },
        })
        .pipe(defaultIfEmpty(false)),
    )
  }
}
