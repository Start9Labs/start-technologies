import { Component } from '@angular/core'
import { TuiButton, TuiDialogContext, TuiIcon, TuiTitle } from '@taiga-ui/core'
import { TuiCopy, TuiSegmented, TuiTextarea } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { QrCodeComponent } from 'ng-qrcode'
import { provideHelp } from 'src/app/help/help'
import { ModalHelp } from 'src/app/help/modal-help'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    <header tuiHeader>
      <h2 tuiTitle>{{ 'Device Config' | i18n }}</h2>
      <aside tuiAccessories>
        <tui-segmented #segmented>
          <button>
            <tui-icon icon="@tui.file" />
            {{ 'File' | i18n }}
          </button>
          <button>
            <tui-icon icon="@tui.qr-code" />
            {{ 'QR' | i18n }}
          </button>
        </tui-segmented>
      </aside>
    </header>
    @if (segmented?.activeItemIndex()) {
      <qr-code [value]="config" size="352" />
    } @else {
      <tui-textfield>
        <textarea
          tuiTextarea
          [min]="16"
          [max]="16"
          [readOnly]="true"
          [value]="config"
        ></textarea>
        <tui-icon tuiCopy />
        <a
          tuiIconButton
          iconStart="@tui.download"
          download="start-tunnel.conf"
          size="s"
          [href]="href"
        >
          {{ 'Download' | i18n }}
        </a>
      </tui-textfield>
    }
  `,
  imports: [
    QrCodeComponent,
    TuiButton,
    TuiHeader,
    TuiIcon,
    TuiTitle,
    TuiSegmented,
    TuiTextarea,
    TuiCopy,
    i18nPipe,
  ],
  hostDirectives: [ModalHelp],
  providers: [provideHelp('/devices/config')],
})
export class DevicesConfig {
  protected readonly config =
    injectContext<TuiDialogContext<void, string>>().data
  protected readonly href = URL.createObjectURL(
    new Blob([this.config], { type: 'application/octet-stream' }),
  )
}

export const DEVICES_CONFIG = new PolymorpheusComponent(DevicesConfig)
