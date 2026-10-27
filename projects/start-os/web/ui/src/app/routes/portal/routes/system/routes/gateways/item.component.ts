import { Component, inject, input } from '@angular/core'
import { DialogService, i18nPipe, TaskService } from '@start9labs/shared'
import { ISB } from '@start9labs/start-core'
import { TuiButton, TuiDataList, TuiDropdown, TuiIcon } from '@taiga-ui/core'
import { filter } from 'rxjs'
import { FormComponent } from 'src/app/routes/portal/components/form.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { FormDialogService } from 'src/app/services/form-dialog.service'
import { GatewayPlus } from 'src/app/services/gateway.service'
import { configBuilderToSpec } from 'src/app/utils/configBuilderToSpec'

import EditGateway from './edit'
import { PORT_FORWARDS_MODAL } from './port-forwards.component'

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
          gateway.ipInfo.wanIp ? undefined : 'var(--tui-text-warning)'
        "
      >
        {{ gateway.ipInfo.wanIp || ('Error' | i18n) }}
      </td>
      <td [attr.data-label]="'DNS' | i18n">
        {{ gateway.dns.servers.join(', ') }}
        @if (gateway.dns.source === 'gateway') {
          ({{ 'Auto' | i18n }})
        } @else if (gateway.dns.source === 'public-fallback') {
          ({{ 'Fallback' | i18n }})
        }
      </td>
      <td>
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
            <button tuiOption (click)="edit()">
              {{ 'Edit' | i18n }}
            </button>
            @if (gateway.type !== 'outbound-only') {
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

    td:last-child {
      text-align: right;
    }

    td[data-label] {
      overflow-wrap: anywhere;
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
      td:nth-child(4),
      td:nth-child(5) {
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

      td:nth-child(5)::before {
        content: attr(data-label) ': ';
      }

      td:last-child {
        grid-area: 1 / 3 / 6;
      }
    }
  `,
  imports: [TuiButton, TuiDropdown, TuiDataList, TuiIcon, i18nPipe],
})
export class GatewaysItemComponent {
  private readonly dialog = inject(DialogService)
  private readonly tasks = inject(TaskService)
  private readonly api = inject(ApiService)
  private readonly formDialog = inject(FormDialogService)
  private readonly i18n = inject(i18nPipe)

  public readonly gateway = input.required<GatewayPlus>()

  edit() {
    this.formDialog.open(EditGateway, { size: 'm', data: this.gateway().id })
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
          async () =>
            await this.api.rpc.net.tunnel.remove({ id: this.gateway().id }),
          'Deleting',
        ),
      )
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
                  await this.api.rpc.net.tunnel.update({
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
}
