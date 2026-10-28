import { inject, Injectable } from '@angular/core'
import { Router } from '@angular/router'
import {
  DialogService,
  i18nKey,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { PatchDB } from 'patch-db-client'
import { filter } from 'rxjs'
import { getAllPackages } from '../utils/get-package-data'
import { hasCurrentDeps } from '../utils/has-deps'
import { ApiService } from './api/embassy-api.service'
import { DataModel } from './patch-db/data-model'

@Injectable({
  providedIn: 'root',
})
export class StandardActionsService {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly api = inject(ApiService)
  private readonly dialog = inject(DialogService)
  private readonly tasks = inject(TaskService)
  private readonly router = inject(Router)
  private readonly i18n = inject(i18nPipe)

  async rebuild(id: string) {
    this.tasks.run(async () => {
      await this.api.rpc.package.rebuild({ id })
      await this.router.navigate(['services', id])
    }, 'Rebuilding container')
  }

  async uninstall(
    { id, title }: T.Manifest,
    { force, soft }: { force: boolean; soft: boolean } = {
      force: false,
      soft: false,
    },
  ): Promise<void> {
    let content = soft
      ? ''
      : `${this.i18n.transform('Uninstalling')} ${title} ${this.i18n.transform('will permanently delete its data.')}`

    if (hasCurrentDeps(id, await getAllPackages(this.patch))) {
      content = `${content ? `${content} ` : ''}${this.i18n.transform('Services that depend on')} ${title} ${this.i18n.transform('will no longer work properly and may crash.')}`
    }

    if (!content) {
      return this.doUninstall({ id, force, soft })
    }

    this.dialog
      .openConfirm({
        label: 'Warning',
        size: 's',
        data: {
          content: content as i18nKey,
          yes: 'Uninstall',
          no: 'Cancel',
        },
      })
      .pipe(filter(Boolean))
      .subscribe(() => this.doUninstall({ id, force, soft }))
  }

  private async doUninstall(options: T.UninstallParams) {
    this.tasks.run(async () => {
      await this.api.rpc.package.uninstall(options)
      await this.router.navigate([''])
    }, 'Beginning uninstall')
  }
}
