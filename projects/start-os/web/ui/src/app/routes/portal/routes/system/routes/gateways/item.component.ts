import { Component, inject, Injector, input } from '@angular/core'
import { DialogService, i18nPipe, TaskService } from '@start9labs/shared'
import { ISB } from '@start9labs/start-core'
import {
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiHint,
  TuiIcon,
  TuiInput,
} from '@taiga-ui/core'
import { PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { filter } from 'rxjs'
import { FormComponent } from 'src/app/routes/portal/components/form.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { FormDialogService } from 'src/app/services/form-dialog.service'
import { GatewayPlus } from 'src/app/services/gateway.service'
import { configBuilderToSpec } from 'src/app/utils/configBuilderToSpec'
import { PORT_FORWARDS_MODAL } from './port-forwards.component'
import { WanIpDialog } from './wan-ip.dialog'

@Component({
  selector: 'tr[gateway]',
  template: `
    @if (gateway(); as gateway) {
      <td>
        @switch (gateway.ipInfo.deviceType) {
          @case ('ethernet') {
            <tui-icon icon="@tui.ethernet-port" />
          }
          @case ('wireless') {
            <tui-icon icon="@tui.wifi" />
          }
          @case ('wireguard') {
            <tui-icon icon="@tui.shield" />
          }
        }
        {{ gateway.name }}
      </td>
      <td>
        @if (gateway.type === 'outbound-only') {
          {{ 'Outbound Only' | i18n }}
        } @else {
          {{ 'Inbound/Outbound' | i18n }}
        }
      </td>
      <td class="lan">{{ gateway.lanIpv4.join(', ') || '-' }}</td>
      <td
        class="wan"
        [style.color]="
          gateway.effectiveWanIp ? undefined : 'var(--tui-text-warning)'
        "
      >
        {{ gateway.effectiveWanIp || ('Error' | i18n) }}
        @if (gateway.wanIpOverride) {
          <tui-icon icon="@tui.pencil" [tuiHint]="'Set manually' | i18n" />
        }
      </td>
      <td>
        <button
          tuiIconButton
          tuiDropdown
          size="s"
          appearance="flat-grayscale"
          iconStart="@tui.ellipsis-vertical"
          [tuiAppearanceState]="open ? 'hover' : null"
          [(tuiDropdownOpen)]="open"
        >
          {{ 'More' | i18n }}
          <tui-data-list *tuiDropdown>
            <button tuiOption (click)="rename()">
              {{ 'Rename' | i18n }}
            </button>
            @if (gateway.type !== 'outbound-only') {
              <button tuiOption (click)="editWanIp()">
                {{ 'Edit WAN IP' | i18n }}
              </button>
              <button tuiOption (click)="viewPortForwards()">
                {{ 'View port forwards' | i18n }}
              </button>
            }
            @if (gateway.ipInfo.deviceType === 'wireguard') {
              <button tuiOption (click)="updateConfig()">
                {{ 'Update config' | i18n }}
              </button>
              <button tuiOption class="g-negative" (click)="remove()">
                {{ 'Delete' | i18n }}
              </button>
            }
          </tui-data-list>
        </button>
      </td>
    }
  `,
  styles: `
    tui-icon {
      font-size: 1.3rem;
      margin-right: 0.7rem;
    }

    .wan tui-icon {
      font-size: 1rem;
      margin: 0 0 0 0.25rem;
      color: var(--tui-text-secondary);
    }

    td:first-child {
      width: 24rem;
    }

    td:last-child {
      text-align: right;
    }

    :host-context(tui-root._mobile) {
      td {
        width: auto !important;
        align-content: center;
      }

      td:first-child {
        font: var(--tui-typography-body-m);
        font-weight: bold;
        color: var(--tui-text-primary);
      }

      td:nth-child(2) {
        grid-area: 2 / 1 / 2 / 3;
      }

      td:nth-child(3),
      td:nth-child(4) {
        grid-area: auto / 1 / auto / 3;

        &::before {
          color: var(--tui-text-primary);
        }
      }

      td:nth-child(3)::before {
        content: 'LAN IP: ';
      }

      td:nth-child(4)::before {
        content: 'WAN IP: ';
      }

      td:last-child {
        grid-area: 1 / 3 / 6;
      }
    }
  `,
  imports: [
    TuiButton,
    TuiDropdown,
    TuiDataList,
    TuiHint,
    TuiIcon,
    TuiInput,
    i18nPipe,
  ],
})
export class GatewaysItemComponent {
  private readonly dialog = inject(DialogService)
  private readonly tasks = inject(TaskService)
  private readonly api = inject(ApiService)
  private readonly formDialog = inject(FormDialogService)
  private readonly i18n = inject(i18nPipe)
  private readonly injector = inject(Injector)

  readonly gateway = input.required<GatewayPlus>()

  open = false

  editWanIp() {
    const { id, wanIpOverride, ipInfo } = this.gateway()

    this.dialog
      .openComponent<string | null>(
        new PolymorpheusComponent(WanIpDialog, this.injector),
        {
          label: 'Edit WAN IP',
          size: 's',
          data: { detected: ipInfo.wanIp, override: wanIpOverride },
        },
      )
      .subscribe(ip =>
        this.tasks.run(
          async () =>
            ip === null
              ? await this.api.unsetGatewayWanIp({ gateway: id })
              : await this.api.setGatewayWanIp({ gateway: id, ip }),
          'Saving',
        ),
      )
  }

  viewPortForwards() {
    const { id, name } = this.gateway()
    this.dialog
      .openComponent(PORT_FORWARDS_MODAL, {
        label: 'Port Forwards',
        size: 'l',
        data: { gatewayId: id, gatewayName: name },
      })
      .subscribe()
  }

  remove() {
    this.dialog
      .openConfirm({ label: 'Are you sure?', size: 's' })
      .pipe(filter(Boolean))
      .subscribe(() =>
        this.tasks.run(
          async () => await this.api.removeTunnel({ id: this.gateway().id }),
          'Deleting',
        ),
      )
  }

  async rename() {
    const { id, name } = this.gateway()
    const renameSpec = ISB.InputSpec.of({
      label: ISB.Value.text({
        name: this.i18n.transform('Name'),
        required: true,
        default: name,
      }),
    })

    this.formDialog.open(FormComponent, {
      label: 'Rename',
      data: {
        spec: await configBuilderToSpec(renameSpec),
        buttons: [
          {
            text: 'Save',
            handler: (value: typeof renameSpec._TYPE) =>
              this.update(id, value.label),
          },
        ],
      },
    })
  }

  async updateConfig() {
    const { id } = this.gateway()
    const spec = ISB.InputSpec.of({
      config: ISB.Value.union({
        name: this.i18n.transform('WireGuard Config File'),
        default: 'paste',
        variants: ISB.Variants.of({
          paste: {
            name: this.i18n.transform('Copy/Paste'),
            spec: ISB.InputSpec.of({
              file: ISB.Value.textarea({
                name: this.i18n.transform('File Contents'),
                default: null,
                required: true,
                minRows: 8,
                maxRows: 8,
              }),
            }),
          },
          select: {
            name: this.i18n.transform('Upload'),
            spec: ISB.InputSpec.of({
              file: ISB.Value.file({
                name: this.i18n.transform('File'),
                required: true,
                extensions: ['.conf'],
              }),
            }),
          },
        }),
      }),
    })

    this.formDialog.open(FormComponent, {
      label: 'Update Gateway Config',
      data: {
        spec: await configBuilderToSpec(spec),
        buttons: [
          {
            text: this.i18n.transform('Save'),
            handler: async (input: typeof spec._TYPE) =>
              this.tasks.run(
                async () =>
                  await this.api.updateTunnelConfig({
                    id,
                    config:
                      input.config.selection === 'paste'
                        ? input.config.value.file
                        : await (input.config.value.file as any as File).text(),
                  }),
                'Saving',
              ),
          },
        ],
      },
    })
  }

  private async update(id: string, name: string): Promise<boolean> {
    return this.tasks.run(
      async () => await this.api.updateTunnel({ id, name }),
      'Saving',
    )
  }
}
