import { CommonModule, TitleCasePipe } from '@angular/common'
import { Component, inject } from '@angular/core'
import {
  DialogService,
  i18nPipe,
  LocalizePipe,
  MarkdownPipe,
  SafeLinksDirective,
  TaskService,
} from '@start9labs/shared'
import { TuiButton, TuiDialogContext, TuiScrollbar } from '@taiga-ui/core'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'
import { firstValueFrom } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { OSService } from 'src/app/services/os.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { trustedOsUpdateSigners } from 'src/app/utils/os-update-signers'

@Component({
  template: `
    <h2 style="margin-top: 0">{{ 'Release notes' | i18n | titlecase }}</h2>
    <tui-scrollbar style="margin-bottom: 24px; max-height: 50vh;">
      @for (v of os.updateCandidates(); track v.version) {
        <h4 class="version-header">{{ v.version }}</h4>
        <div
          safeLinks
          [innerHTML]="v.notes | localize | markdown | dompurify"
        ></div>
      }
    </tui-scrollbar>
    <button
      tuiButton
      style="float: right;"
      [disabled]="!os.updateCandidates().length"
      (click)="update()"
    >
      {{ 'Begin Update' | i18n }}
    </button>
  `,
  styles: `
    .version-header {
      font-weight: bold;
      font-size: 1.4rem;
      margin: 2rem 0 0 0;
      color: var(--tui-text-secondary);
    }
  `,
  imports: [
    CommonModule,
    LocalizePipe,
    MarkdownPipe,
    NgDompurifyPipe,
    SafeLinksDirective,
    TuiButton,
    TuiScrollbar,
    i18nPipe,
    TitleCasePipe,
  ],
})
export class SystemUpdateModal {
  private readonly tasks = inject(TaskService)
  private readonly dialogs = inject(DialogService)
  private readonly embassyApi = inject(ApiService)
  protected readonly os = inject(OSService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly context = injectContext<TuiDialogContext<void>>()

  async update() {
    if (!this.os.updateCandidates().length) return

    const { startosRegistry } = await firstValueFrom(this.patch.watch$('ui'))

    this.tasks.run(async () => {
      const latest = this.os.updateCandidates()[0]
      if (!latest) return

      const { os } = await this.embassyApi.rpc.server.deviceInfo({})
      const signers = Object.keys(
        latest.squashfs[os.platform]?.signatures ?? {},
      )
      const trustedSigners = await firstValueFrom(
        this.patch.watch$('serverInfo', 'trustedOsUpdateSigners'),
      )
      const trusted = trustedOsUpdateSigners(signers, trustedSigners)
      if (this.os.updateCandidates()[0]?.version !== latest.version) return
      if (!trusted.length) {
        this.dialogs
          .openAlert(
            'No trusted signer is listed for this OS update. Installing it from the UI is blocked. A malicious OS update could take full control of your server, steal private keys and passwords, or permanently destroy your data.',
            { label: 'OS update blocked' },
          )
          .subscribe()
        return
      }

      await this.embassyApi.rpc.server.update({
        targetVersion: `=${latest.version}`,
        registry: startosRegistry,
        progress: false,
        acceptSigners: {
          any: trusted.map(signer => ({ signer })),
        },
      })
      this.context.$implicit.complete()
    }, 'Beginning update')
  }
}

export const UPDATE = new PolymorpheusComponent(SystemUpdateModal)
