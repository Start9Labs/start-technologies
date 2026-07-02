import { WA_IS_MOBILE } from '@ng-web-apis/platform'
import { Component, inject, input, model, signal } from '@angular/core'
import { CopyService, DialogService, i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TuiButton,
  tuiButtonOptionsProvider,
  TuiDataList,
  TuiDropdown,
  TuiInput,
} from '@taiga-ui/core'
import { PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { QRModal } from 'src/app/routes/portal/modals/qr.component'
import { ActionService } from 'src/app/services/action.service'
import {
  MappedServiceInterface,
  PluginAddress,
  PluginAddressGroup,
} from '../../interface.service'

@Component({
  selector: 'td[pluginActions]',
  template: `
    @if (address(); as addr) {
      <div class="desktop">
        @if (addr.hostnameInfo.metadata.kind === 'plugin') {
          @if (addr.hostnameInfo.metadata.removeAction) {
            @if (
              pluginGroup().pluginActions[
                addr.hostnameInfo.metadata.removeAction
              ];
              as meta
            ) {
              <button
                tuiIconButton
                appearance="flat-grayscale"
                iconStart="@tui.trash"
                (click)="
                  runRowAction(addr.hostnameInfo.metadata.removeAction, meta)
                "
              >
                {{ meta.name }}
              </button>
            }
          }
        }
        <button
          tuiIconButton
          appearance="flat-grayscale"
          iconStart="@tui.qr-code"
          (click)="showQR()"
        >
          {{ 'Show QR' | i18n }}
        </button>
        <button
          tuiIconButton
          appearance="flat-grayscale"
          iconStart="@tui.copy"
          (click)="copyService.copy(addr.url)"
        >
          {{ 'Copy URL' | i18n }}
        </button>
        @if (addr.hostnameInfo.metadata.kind === 'plugin') {
          @if (addr.hostnameInfo.metadata.overflowActions.length) {
            <button
              tuiIconButton
              tuiDropdown
              appearance="flat-grayscale"
              iconStart="@tui.ellipsis-vertical"
              [tuiAppearanceState]="overflowOpen() ? 'hover' : null"
              [(tuiDropdownOpen)]="overflowOpen"
            >
              {{ 'More' | i18n }}
              <tui-data-list *tuiDropdown (click)="overflowOpen.set(false)">
                @for (
                  actionId of addr.hostnameInfo.metadata.overflowActions;
                  track actionId
                ) {
                  @if (pluginGroup().pluginActions[actionId]; as meta) {
                    <button
                      tuiOption
                      iconStart="@tui.play"
                      (click)="runRowAction(actionId, meta)"
                    >
                      {{ meta.name }}
                    </button>
                  }
                }
              </tui-data-list>
            </button>
          }
        }
      </div>
      <div class="mobile">
        <button
          tuiDropdown
          tuiIconButton
          appearance="flat-grayscale"
          iconStart="@tui.ellipsis-vertical"
          [tuiAppearanceState]="mobileOpen() ? 'hover' : null"
          [(tuiDropdownOpen)]="mobileOpen"
        >
          {{ 'Actions' | i18n }}
          <tui-data-list *tuiDropdown (click)="mobileOpen.set(false)">
            @if (addr.masked) {
              <button
                tuiOption
                [iconStart]="currentlyMasked() ? '@tui.eye' : '@tui.eye-off'"
                (click)="currentlyMasked.set(!currentlyMasked())"
              >
                {{ (currentlyMasked() ? 'Reveal' : 'Hide') | i18n }}
              </button>
            }
            <button tuiOption iconStart="@tui.qr-code" (click)="showQR()">
              {{ 'Show QR' | i18n }}
            </button>
            <button
              tuiOption
              iconStart="@tui.copy"
              (click)="copyService.copy(addr.url)"
            >
              {{ 'Copy URL' | i18n }}
            </button>
            @if (addr.hostnameInfo.metadata.kind === 'plugin') {
              @for (
                actionId of addr.hostnameInfo.metadata.overflowActions;
                track actionId
              ) {
                @if (pluginGroup().pluginActions[actionId]; as meta) {
                  <button
                    tuiOption
                    iconStart="@tui.play"
                    (click)="runRowAction(actionId, meta)"
                  >
                    {{ meta.name }}
                  </button>
                }
              }
              @if (addr.hostnameInfo.metadata.removeAction) {
                @if (
                  pluginGroup().pluginActions[
                    addr.hostnameInfo.metadata.removeAction
                  ];
                  as meta
                ) {
                  <button
                    tuiOption
                    iconStart="@tui.trash"
                    (click)="
                      runRowAction(
                        addr.hostnameInfo.metadata.removeAction,
                        meta
                      )
                    "
                  >
                    {{ meta.name }}
                  </button>
                }
              }
            }
          </tui-data-list>
        </button>
      </div>
    }
  `,
  styles: `
    :host {
      text-align: right;
      width: fit-content;
      place-content: center;
      white-space: nowrap;
    }

    .desktop {
      display: flex;
      white-space: nowrap;
    }

    .mobile {
      display: none;
    }

    :host-context(tui-root._mobile) {
      .desktop {
        display: none;
      }

      .mobile {
        display: block;
      }
    }
  `,
  imports: [TuiButton, TuiDropdown, TuiDataList, TuiInput, i18nPipe],
  providers: [tuiButtonOptionsProvider({ appearance: 'icon' })],
})
export class PluginActionsComponent {
  private readonly isMobile = inject(WA_IS_MOBILE)
  private readonly dialog = inject(DialogService)
  private readonly actionService = inject(ActionService)
  readonly copyService = inject(CopyService)

  readonly currentlyMasked = model(true)
  readonly overflowOpen = signal(false)
  readonly mobileOpen = signal(false)

  readonly address = input.required<PluginAddress>()
  readonly pluginGroup = input.required<PluginAddressGroup>()
  readonly packageId = input('')
  readonly value = input<MappedServiceInterface | undefined>()

  showQR() {
    this.dialog
      .openComponent(new PolymorpheusComponent(QRModal), {
        size: 's',
        closable: this.isMobile,
        data: this.address().url,
      })
      .subscribe()
  }

  runRowAction(actionId: string, metadata: T.ActionMetadata) {
    const group = this.pluginGroup()
    if (!group.pluginPkgInfo) return

    const iface = this.value()
    if (!iface) return

    const { hostnameInfo } = this.address()
    const { addressInfo } = iface
    const hostMeta = hostnameInfo.metadata

    if (hostMeta.kind !== 'plugin') return

    this.actionService.present({
      pkgInfo: group.pluginPkgInfo,
      actionInfo: { id: actionId, metadata },
      prefill: {
        urlPluginMetadata: {
          packageId: this.packageId(),
          hostId: addressInfo.hostId,
          interfaceId: iface.id,
          internalPort: addressInfo.internalPort,
          hostname: hostnameInfo.hostname,
          port: hostnameInfo.port,
          ssl: hostnameInfo.ssl,
          public: hostnameInfo.public,
          info: hostMeta.info,
        },
      },
    })
  }
}
