import { CommonModule } from '@angular/common'
import { Component, computed, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { Router } from '@angular/router'
import {
  acceptSignerPolicy,
  MarketplaceSignerChipsComponent,
} from '@start9labs/marketplace'
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
import { catchError, filter, firstValueFrom, from, map, of } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { OSService } from 'src/app/services/os.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { trustedOsUpdateSigners } from 'src/app/utils/os-update-signers'
import { TRUSTED_KEYS_LINK } from '../../system.const'

@Component({
  template: `
    <h2>StartOS {{ os.updateCandidates()[0]?.version }}</h2>
    <tui-scrollbar style="margin-bottom: 24px; max-height: 50vh;">
      @if (signers().length) {
        <h3>{{ 'Signed by' | i18n }}</h3>
        <marketplace-signer-chips [keys]="signers()" context="os" />
      }
      <h3>{{ 'Release notes' | i18n }}</h3>
      @for (v of os.updateCandidates(); track v.version) {
        @if (os.updateCandidates().length > 1) {
          <h4>{{ v.version }}</h4>
        }
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
    h2 {
      margin-top: 0;
    }

    h3 {
      margin: 1.5rem 0 0.75rem;
      font: var(--tui-typography-heading-h6);

      &:first-child {
        margin-top: 0;
      }
    }

    h4 {
      margin: 1.25rem 0 0;
      font: var(--tui-typography-body-l);
      font-weight: bold;
      color: var(--tui-text-secondary);
    }
  `,
  imports: [
    CommonModule,
    LocalizePipe,
    MarkdownPipe,
    MarketplaceSignerChipsComponent,
    NgDompurifyPipe,
    SafeLinksDirective,
    TuiButton,
    TuiScrollbar,
    i18nPipe,
  ],
})
export class SystemUpdateModal {
  private readonly tasks = inject(TaskService)
  private readonly dialogs = inject(DialogService)
  private readonly router = inject(Router)
  private readonly embassyApi = inject(ApiService)
  protected readonly os = inject(OSService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly context = injectContext<TuiDialogContext<void>>()
  private readonly platform = toSignal(
    from(this.embassyApi.rpc.server.deviceInfo({})).pipe(
      map(({ os }) => os.platform),
      catchError(() => of(null)),
    ),
  )

  protected readonly signers = computed(() =>
    Object.keys(
      this.os.updateCandidates()[0]?.squashfs[this.platform() ?? '']
        ?.signatures ?? {},
    ),
  )

  async update() {
    if (!this.os.updateCandidates().length) return

    const { osRegistry } = await firstValueFrom(this.patch.watch$('serverInfo'))

    this.tasks.run(async () => {
      const latest = this.os.updateCandidates()[0]
      if (!latest) return

      const { os } = await this.embassyApi.rpc.server.deviceInfo({})
      const signers = Object.keys(
        latest.squashfs[os.platform]?.signatures ?? {},
      )
      const trustedSigners = await firstValueFrom(
        this.patch.watch$('serverInfo', 'trustedKeys'),
      )
      const trusted = trustedOsUpdateSigners(signers, trustedSigners)
      if (this.os.updateCandidates()[0] !== latest) return
      if (!trusted.length) {
        this.dialogs
          .openConfirm({
            label: 'OS update blocked',
            data: {
              content:
                'No trusted signer is listed for this OS update. Installing it from the UI is blocked. A malicious OS update could take full control of your server, steal private keys and passwords, or permanently destroy your data.',
              yes: 'Manage keys',
              no: 'Close',
            },
          })
          .pipe(filter(Boolean))
          .subscribe(() => {
            this.context.$implicit.complete()
            this.router.navigate(TRUSTED_KEYS_LINK)
          })
        return
      }

      await this.embassyApi.rpc.server.update({
        targetVersion: `=${latest.version}`,
        registry: osRegistry,
        progress: false,
        acceptSigners: acceptSignerPolicy(trusted),
      })
      this.context.$implicit.complete()
    }, 'Beginning update')
  }
}

export const UPDATE = new PolymorpheusComponent(SystemUpdateModal)
