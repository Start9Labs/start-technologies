import { Component, inject, Injectable } from '@angular/core'
import {
  DialogService,
  Exver,
  i18nKey,
  i18nPipe,
  LocalizePipe,
  MarkdownPipe,
  SafeLinksDirective,
  sameUrl,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiDialogContext, TuiNotification } from '@taiga-ui/core'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { TuiConfirmData } from '@taiga-ui/kit'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { defaultIfEmpty, firstValueFrom } from 'rxjs'
import { MarketplaceService } from 'src/app/services/marketplace.service'

type PreDownloadDialogData = TuiConfirmData & {
  content: PolymorpheusComponent<PreDownloadMessage>
  message: T.LocaleString
}

@Component({
  template: `
    <div tuiNotification appearance="warning">
      <div
        class="g-markdown"
        safeLinks
        [innerHTML]="message | localize | markdown | dompurify"
      ></div>
    </div>
  `,
  imports: [
    LocalizePipe,
    MarkdownPipe,
    NgDompurifyPipe,
    SafeLinksDirective,
    TuiNotification,
  ],
})
class PreDownloadMessage {
  protected readonly message =
    injectContext<TuiDialogContext<boolean, PreDownloadDialogData>>().data
      .message
}

const PRE_DOWNLOAD_MESSAGE = new PolymorpheusComponent(PreDownloadMessage)

@Injectable({
  providedIn: 'root',
})
export class MarketplaceAlertsService {
  private readonly dialog = inject(DialogService)
  private readonly marketplaceService = inject(MarketplaceService)
  private readonly i18n = inject(i18nPipe)
  private readonly exver = inject(Exver)

  async alertMarketplace(
    url: string,
    originalUrl: string | null,
  ): Promise<boolean> {
    const registries = await firstValueFrom(this.marketplaceService.registries$)
    const message = originalUrl
      ? `${this.i18n.transform('installed from')} ${registries.find(r => sameUrl(r.url, originalUrl))?.name || originalUrl}`
      : this.i18n.transform('sideloaded')

    const currentName = registries.find(h => sameUrl(h.url, url))?.name || url

    return new Promise(async resolve => {
      this.dialog
        .openConfirm({
          label: 'Warning',
          size: 's',
          data: {
            content:
              `${this.i18n.transform('This service was originally')} ${message}, ${this.i18n.transform('but you are currently connected to')} ${currentName}. ${this.i18n.transform('To install from')} ${currentName} ${this.i18n.transform('anyway, click "Continue".')}` as i18nKey,
            yes: 'Continue',
            no: 'Cancel',
          },
        })
        .pipe(defaultIfEmpty(false))
        .subscribe(response => resolve(response))
    })
  }

  async alertPreDownload(
    alert: T.PreDownloadAlert | null | undefined,
    sourceVersion: string | null,
  ): Promise<boolean> {
    if (
      !alert ||
      !sourceVersion ||
      !this.exver.satisfies(sourceVersion, alert.when.sourceVersion)
    ) {
      return true
    }

    const data: PreDownloadDialogData = {
      content: PRE_DOWNLOAD_MESSAGE,
      message: alert.message,
      yes: 'Continue',
      no: 'Cancel',
    }

    return firstValueFrom(
      this.dialog
        .openConfirm({ label: 'Wait!', size: 's', data })
        .pipe(defaultIfEmpty(false)),
    )
  }

  async alertBreakages(breakages: string[]): Promise<boolean> {
    let content =
      `${this.i18n.transform('As a result of this update, the following services will no longer work properly and may crash')}:<ul>` as i18nKey
    const bullets = breakages.map(title => `<li><b>${title}</b></li>`)
    content = `${content}${bullets.join('')}</ul>` as i18nKey

    return new Promise(async resolve => {
      this.dialog
        .openConfirm({
          label: 'Warning',
          size: 's',
          data: {
            content,
            yes: 'Continue',
            no: 'Cancel',
          },
        })
        .pipe(defaultIfEmpty(false))
        .subscribe(response => resolve(response))
    })
  }
}
