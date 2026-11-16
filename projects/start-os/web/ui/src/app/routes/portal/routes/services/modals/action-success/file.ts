import { Component, inject, input } from '@angular/core'
import { i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiTitle } from '@taiga-ui/core'
import { ApiService } from 'src/app/services/api/embassy-api.service'

@Component({
  selector: 'app-action-success-file',
  template: `
    @if (name()) {
      <span tuiTitle>
        <b>{{ name() }}</b>
        @if (description()) {
          <span tuiSubtitle>{{ description() }}</span>
        }
      </span>
    }
    <a
      tuiButton
      appearance="secondary"
      iconStart="@tui.download"
      [href]="api.downloadFileUrl(file().value)"
      download
    >
      {{ 'Download' | i18n }}
    </a>
  `,
  imports: [TuiButton, TuiTitle, i18nPipe],
})
export class ActionSuccessFile {
  protected readonly api = inject(ApiService)

  public readonly file = input.required<
    T.ActionResultValue & { type: 'file' }
  >()
  public readonly name = input('')
  public readonly description = input('')
}
