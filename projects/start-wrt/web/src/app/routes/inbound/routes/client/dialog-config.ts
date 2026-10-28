import { Component, signal } from '@angular/core'
import {
  TuiButton,
  TuiDialogContext,
  TuiIcon,
  TuiNotification,
  TuiTextfield,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiCopy, TuiSegmented, TuiTextarea } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { QrCodeComponent } from 'ng-qrcode'
import { provideHelp } from 'src/app/help/help'
import { ModalHelp } from 'src/app/help/modal-help'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

export interface ClientConfigData {
  name: string
  config: string
  placeholderKey?: boolean
}

@Component({
  template: `
    <header tuiHeader [style.margin-block-start.rem]="-0.5">
      <h2 tuiTitle [id]="context.id">{{ context.data.name }}</h2>
      <aside tuiAccessories>
        <button
          tuiIconButton
          appearance="icon"
          size="s"
          [iconStart]="masked() ? '@tui.eye' : '@tui.eye-off'"
          (click)="masked.set(!masked())"
        >
          {{ 'Toggle visibility' | i18n }}
        </button>
        @if (!context.data.placeholderKey) {
          <tui-segmented [(activeItemIndex)]="index">
            <button>
              <tui-icon icon="@tui.file" />
              {{ 'File' | i18n }}
            </button>
            <button>
              <tui-icon icon="@tui.qr-code" />
              {{ 'QR' | i18n }}
            </button>
          </tui-segmented>
        }
      </aside>
    </header>
    @if (context.data.placeholderKey) {
      <div tuiNotification appearance="info">
        {{
          'Replace the PrivateKey placeholder with the private key of this device before using this configuration.'
            | i18n
        }}
      </div>
    }
    @if (index()) {
      <qr-code
        [value]="context.data.config"
        size="352"
        [class.masked]="masked()"
      />
    } @else {
      <tui-textfield [class.masked]="masked()">
        <textarea
          tuiTextarea
          [minRows]="19"
          [maxRows]="19"
          [readonly]="true"
          [value]="context.data.config"
        ></textarea>
        <tui-icon tuiCopy />
        <a
          tuiIconButton
          iconStart="@tui.download"
          download="wireguard.conf"
          size="s"
          [href]="href"
        >
          {{ 'Download' | i18n }}
        </a>
      </tui-textfield>
    }
  `,
  styles: `
    header {
      flex-wrap: wrap;
    }

    [tuiNotification] {
      margin-block-end: 1rem;
    }

    .masked {
      filter: blur(8px);
      pointer-events: none;
      user-select: none;
    }
  `,
  hostDirectives: [ModalHelp],
  providers: [provideHelp('/inbound/client/dialog-config')],
  imports: [
    QrCodeComponent,
    TuiButton,
    TuiHeader,
    TuiIcon,
    TuiTitle,
    TuiSegmented,
    TuiNotification,
    TuiTextfield,
    TuiTextarea,
    TuiCopy,
    i18nPipe,
  ],
})
export class ClientConfigDialog {
  protected readonly index = signal(0)
  protected readonly masked = signal(true)
  protected readonly context =
    injectContext<TuiDialogContext<void, ClientConfigData>>()
  protected readonly href = URL.createObjectURL(
    new Blob([this.context.data.config], { type: 'application/octet-stream' }),
  )
}

export const CLIENT_CONFIG = new PolymorpheusComponent(ClientConfigDialog)
