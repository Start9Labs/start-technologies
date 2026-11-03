import {
  Component,
  computed,
  DOCUMENT,
  inject,
  linkedSignal,
  signal,
} from '@angular/core'
import { FormsModule } from '@angular/forms'
import { TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TuiButton,
  TuiDialogContext,
  TuiIcon,
  TuiInput,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiCopy, TuiSegmented, TuiTextarea } from '@taiga-ui/kit'
import { TuiForm, TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { QrCodeComponent } from 'ng-qrcode'
import { provideHelp } from 'src/app/help/help'
import { ModalHelp } from 'src/app/help/modal-help'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { ApiService } from 'src/app/services/api/api.service'

@Component({
  template: `
    <header tuiHeader>
      <h2 tuiTitle [id]="context.id">{{ 'Device Config' | i18n }}</h2>
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
    <form tuiForm="m" (submit.prevent)="generate()">
      <tui-textfield>
        <label tuiLabel>{{ 'WireGuard endpoint' | i18n }}</label>
        <input
          tuiInput
          name="endpoint"
          required
          [(ngModel)]="endpoint"
          [disabled]="generating()"
        />
      </tui-textfield>
      <p>
        {{
          'Defaults to this page’s host. Use an IP address or DNS name reachable by the device; the WireGuard port is unchanged.'
            | i18n
        }}
      </p>
      <footer>
        <button tuiButton [disabled]="!endpoint() || generating()">
          {{ 'Generate config' | i18n }}
        </button>
      </footer>
    </form>
    @if (config()) {
      @if (segmented.activeItemIndex()) {
        <qr-code [value]="config()" size="352" />
      } @else {
        <tui-textfield>
          <textarea
            tuiTextarea
            [minRows]="16"
            [maxRows]="16"
            [readonly]="true"
            [value]="config()"
          ></textarea>
          <tui-icon tuiCopy />
          <a
            tuiIconButton
            iconStart="@tui.download"
            download="start-tunnel.conf"
            size="s"
            [href]="href()"
          >
            {{ 'Download' | i18n }}
          </a>
        </tui-textfield>
      }
    }
  `,
  imports: [
    FormsModule,
    QrCodeComponent,
    TuiButton,
    TuiForm,
    TuiHeader,
    TuiIcon,
    TuiInput,
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
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)

  protected readonly context =
    injectContext<
      TuiDialogContext<
        void,
        Pick<T.Tunnel.ShowConfigParamsInput, 'subnet' | 'ip'>
      >
    >()
  protected readonly endpoint = signal(inject(DOCUMENT).location.hostname)
  protected readonly config = linkedSignal(() => {
    this.endpoint()
    return ''
  })
  protected readonly generating = signal(false)
  protected readonly href = computed(
    () =>
      `data:application/octet-stream;charset=utf-8,${encodeURIComponent(this.config())}`,
  )

  constructor() {
    this.generate()
  }

  protected async generate() {
    if (!this.endpoint() || this.generating()) return
    this.generating.set(true)
    try {
      await this.tasks.run(async () => {
        this.config.set(
          await this.api.rpc.device.showConfig({
            ...this.context.data,
            endpoint: this.endpoint(),
          }),
        )
      })
    } finally {
      this.generating.set(false)
    }
  }
}

export const DEVICES_CONFIG = new PolymorpheusComponent(DevicesConfig)
