import { NgTemplateOutlet } from '@angular/common'
import { Component, inject, model } from '@angular/core'
import { DialogService, i18nPipe, TaskService } from '@start9labs/shared'
import { IST } from '@start9labs/start-core'
import {
  TuiButton,
  TuiCell,
  TuiDataList,
  TuiDropdown,
  TuiIcon,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiBadge, TuiBadgedContent, TuiFade } from '@taiga-ui/kit'
import { filter } from 'rxjs'
import {
  FormComponent,
  FormContext,
} from 'src/app/routes/portal/components/form.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { FormDialogService } from 'src/app/services/form-dialog.service'
import { Wifi, WiFiForm } from './utils'
import SystemWifiComponent from './wifi.component'
import { wifiSpec } from './wifi.const'

@Component({
  selector: '[wifi]',
  template: `
    <ng-template #row let-network>
      <tui-badged-content>
        @if (getSignal(network.strength); as signal) {
          <tui-icon
            background="@tui.wifi"
            [icon]="signal.icon"
            [style.background]="'var(--tui-background-neutral-2)'"
            [style.color]="signal.color"
          />
        } @else {
          <tui-icon icon="@tui.wifi-off" />
        }
        @if (network.security.length) {
          <tui-icon
            appearance="action"
            iconStart="@tui.lock"
            size="s"
            tuiBadge
            tuiSlot="bottom"
            [style.color]="getSignal(network.strength)?.color"
          />
        }
      </tui-badged-content>
      <div tuiTitle>
        <strong tuiFade>
          {{ network.ssid }}
        </strong>
      </div>
      @if (network.connected) {
        <span tuiBadge appearance="positive">
          {{ 'Connected' | i18n }}
        </span>
      }
      @if (network.connected === false) {
        <button
          tuiIconButton
          tuiDropdown
          tuiDropdownAlign="end"
          tuiDropdownAuto
          size="s"
          appearance="flat-grayscale"
          iconStart="@tui.ellipsis-vertical"
        >
          {{ 'More' | i18n }}
          <tui-data-list *tuiDropdown="let close" (click)="close()">
            <button tuiOption iconStart="@tui.wifi" (click)="prompt(network)">
              {{ 'Connect' | i18n }}
            </button>
            <button
              tuiOption
              iconStart="@tui.trash"
              class="g-negative"
              (click)="forget(network)"
            >
              {{ 'Forget' | i18n }}
            </button>
          </tui-data-list>
        </button>
      }
    </ng-template>
    @for (network of wifi(); track $index) {
      @if (network.ssid) {
        @if (network.connected === undefined) {
          <button tuiCell (click)="prompt(network)">
            <ng-container
              *ngTemplateOutlet="row; context: { $implicit: network }"
            />
          </button>
        } @else {
          <div tuiCell>
            <ng-container
              *ngTemplateOutlet="row; context: { $implicit: network }"
            />
          </div>
        }
      }
    }
  `,
  styles: `
    :host {
      padding: 0.5rem !important;
      background: var(--tui-background-neutral-1) !important;
    }

    [tuiCell] {
      &:disabled > * {
        opacity: 1;
      }
    }

    tui-icon {
      color: var(--tui-text-tertiary);
    }

    tui-badged-content {
      margin-inline-start: 0.75rem;
      color: transparent;
    }

    [tuiBadge] {
      --tui-stroke-width: 1.5px;
    }
  `,
  imports: [
    NgTemplateOutlet,
    TuiCell,
    TuiTitle,
    TuiBadge,
    TuiButton,
    TuiIcon,
    TuiFade,
    TuiDropdown,
    TuiDataList,
    TuiBadgedContent,
    i18nPipe,
  ],
})
export class WifiTableComponent {
  private readonly tasks = inject(TaskService)
  private readonly dialogs = inject(DialogService)
  private readonly api = inject(ApiService)
  private readonly formDialog = inject(FormDialogService)
  private readonly component = inject(SystemWifiComponent)
  private readonly i18n = inject(i18nPipe)

  readonly wifi = model<readonly Wifi[]>([])

  getSignal(signal: number) {
    if (signal < 5) {
      return null
    }

    if (signal >= 5 && signal < 50) {
      return {
        icon: '@tui.wifi-low',
        color: 'var(--tui-text-negative)',
      }
    }

    return signal >= 50 && signal < 90
      ? {
          icon: '@tui.wifi-high',
          color: 'var(--tui-status-warning)',
        }
      : {
          icon: '@tui.wifi',
          color: 'var(--tui-text-positive)',
        }
  }

  async forget({ ssid }: Wifi): Promise<void> {
    this.tasks.run(async () => {
      await this.api.rpc.wifi.remove({ ssid })
      this.wifi.update(wifi => wifi.filter(network => network.ssid !== ssid))
    }, 'Deleting')
  }

  async prompt(network: Wifi): Promise<void> {
    if (!network.security.length) {
      this.dialogs
        .openConfirm({
          label: `${this.i18n.transform('Connect to')} ${network.ssid}?`,
          size: 's',
        })
        .pipe(filter(Boolean))
        .subscribe(() => this.component.saveAndConnect(network.ssid))
    } else {
      const ssid = wifiSpec.spec['ssid'] as IST.ValueSpecText
      const spec: IST.InputSpec = {
        ...wifiSpec.spec,
        ssid: { ...ssid, disabled: 'ssid', default: network.ssid },
      }

      this.formDialog.open<FormContext<WiFiForm>>(FormComponent, {
        label: 'Password needed',
        data: {
          spec,
          value: { ssid: network.ssid, password: '' },
          buttons: [
            {
              text: this.i18n.transform('Connect')!,
              handler: async ({ password }) =>
                this.component.saveAndConnect(network.ssid, password),
            },
          ],
        },
      })
    }
  }
}
