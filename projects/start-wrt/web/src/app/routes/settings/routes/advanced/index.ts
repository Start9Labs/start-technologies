import { Component, inject } from '@angular/core'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiButton } from '@taiga-ui/core'
import { TUI_CONFIRM } from '@taiga-ui/kit'
import { TuiCardLarge } from '@taiga-ui/layout'
import { filter } from 'rxjs'
import { ActionService } from 'src/app/services/action.service'
import { ApiService } from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    <section class="g-form" tuiCardLarge [style.align-items]="'start'">
      <a tuiButton [href]="luciUrl" target="_blank">
        {{ 'Launch LuCI Interface' | i18n }}
      </a>
      <button tuiButton (click)="downloadDiagnostics()">
        {{ 'Download Support Diagnostics' | i18n }}
      </button>
      <button tuiButton (click)="factoryReset()">
        {{ 'Factory Reset' | i18n }}
      </button>
    </section>
  `,
  host: { class: 'g-page' },
  imports: [TuiCardLarge, TuiButton, i18nPipe],
})
export default class Advanced {
  private readonly api = inject(ApiService)
  private readonly actions = inject(ActionService)
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly i18n = inject(i18nPipe)
  protected readonly luciUrl = '/cgi-bin/luci'

  protected downloadDiagnostics() {
    this.actions.run(
      async () => {
        const { guid, filename } = await this.api.diagnosticsCreate()
        const res = await fetch(`/rest/rpc/${guid}`)
        if (!res.ok)
          throw new Error(
            `${this.i18n.transform('Download failed:')} ${res.statusText}`,
          )
        const blob = await res.blob()
        const url = URL.createObjectURL(blob)
        const a = document.createElement('a')
        a.href = url
        a.download = filename
        a.click()
        URL.revokeObjectURL(url)
      },
      {
        loading: this.i18n.transform('Creating diagnostics bundle...'),
        success: this.i18n.transform('Diagnostics downloaded'),
      },
    )
  }

  protected factoryReset() {
    this.dialogs
      .open(TUI_CONFIRM, {
        label: this.i18n.transform('Factory Reset'),
        data: {
          content: this.i18n.transform(
            'All settings will be erased and the device will reboot. Your WiFi password will be preserved.',
          ),
          yes: this.i18n.transform('Factory Reset'),
          no: this.i18n.transform('Cancel'),
        },
      })
      .pipe(filter(Boolean))
      .subscribe(() => {
        this.actions.run(
          async () => {
            await this.api.systemFactoryReset()
          },
          {
            loading: this.i18n.transform('Resetting device...'),
            success: this.i18n.transform('Factory reset complete'),
            restart: true,
          },
        )
      })
  }
}
