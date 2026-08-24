import { inject, Injectable } from '@angular/core'
import { DialogService, getErrorMessage, i18nKey } from '@start9labs/shared'
import { TuiNotificationMiddleService } from '@taiga-ui/kit'
import { PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { filter } from 'rxjs'
import { ACTION_CONFIRM_MODAL } from 'src/app/routes/portal/routes/services/modals/action-confirm.component'
import {
  ActionInputModal,
  PackageActionData,
} from 'src/app/routes/portal/routes/services/modals/action-input.component'
import { ActionSuccessPage } from 'src/app/routes/portal/routes/services/modals/action-success/action-success.page'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { FormDialogService } from 'src/app/services/form-dialog.service'

@Injectable({
  providedIn: 'root',
})
export class ActionService {
  private readonly api = inject(ApiService)
  private readonly dialog = inject(DialogService)
  private readonly loader = inject(TuiNotificationMiddleService)
  private readonly formDialog = inject(FormDialogService)

  async present(data: PackageActionData) {
    const { pkgInfo, actionInfo } = data

    if (actionInfo.metadata.hasInput) {
      this.formDialog.open<PackageActionData>(ActionInputModal, {
        label: actionInfo.metadata.name as i18nKey,
        data,
      })
    } else {
      if (actionInfo.metadata.warning) {
        this.dialog
          .openComponent<boolean>(ACTION_CONFIRM_MODAL, {
            label: actionInfo.metadata.name as i18nKey,
            size: 's',
            data,
          })
          .pipe(filter(Boolean))
          .subscribe(() => this.execute(pkgInfo.id, null, actionInfo.id))
      } else {
        this.execute(pkgInfo.id, null, actionInfo.id)
      }
    }
  }

  async execute(
    packageId: string,
    eventId: string | null,
    actionId: string,
    input?: object,
  ) {
    const loader = this.loader.open('Loading').subscribe()

    try {
      const res = await this.api.runAction({
        packageId,
        actionId,
        input: input ?? null,
        eventId,
      })

      if (!res) return

      if (res.result || res.message) {
        this.dialog
          .openComponent(new PolymorpheusComponent(ActionSuccessPage), {
            label: res.title as i18nKey,
            data: res,
          })
          .subscribe()
      }
    } catch (e: any) {
      this.dialog
        .openAlert(getErrorMessage(e) as i18nKey, { label: 'Error' })
        .subscribe()
    } finally {
      loader.unsubscribe()
    }
  }
}
