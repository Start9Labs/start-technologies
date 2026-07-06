import { Component, computed, inject, input, signal } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { i18nPipe, TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiDataList, TuiDropdown, TuiIcon } from '@taiga-ui/core'
import { TuiBadge, TuiChevron, TuiSwitch } from '@taiga-ui/kit'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { GatewayAddress, MappedServiceInterface } from '../../interface.service'
import { GatewayActionsComponent } from './actions.component'
import { DomainHealthService } from './domain-health.service'

@Component({
  selector: 'tr[address]',
  host: {
    '[class._disabled]': '!address().enabled',
    '[class._no-cert]': '!showCert()',
  },
  template: `
    @if (address(); as address) {
      <td>
        <input
          type="checkbox"
          tuiSwitch
          size="s"
          [showIcons]="false"
          [disabled]="
            toggling() || address.hostnameInfo.metadata.kind === 'mdns'
          "
          [ngModel]="address.enabled"
          (ngModelChange)="onToggleEnabled()"
        />
      </td>
      <td class="access">
        @if (address.gua) {
          <!-- A GUA's Local vs Public is its WAN opt-in (the public flag), so
               it shows like the other rows but as a dropdown to change it. -->
          <button
            tuiButton
            tuiDropdown
            tuiChevron
            size="s"
            appearance="secondary-grayscale"
            [tuiAppearanceState]="guaOpen() ? 'hover' : null"
            [tuiDropdownOpen]="guaOpen()"
            (tuiDropdownOpenChange)="guaOpen.set($event)"
            [disabled]="toggling()"
            [iconStart]="
              address.access === 'public' ? '@tui.globe' : '@tui.house'
            "
          >
            {{ (address.access === 'public' ? 'Public' : 'Local') | i18n }}
            <tui-data-list *tuiDropdown (click)="guaOpen.set(false)">
              <button
                tuiOption
                iconStart="@tui.house"
                (click)="onSetGuaWan(false)"
              >
                {{ 'Local' | i18n }}
              </button>
              <button
                tuiOption
                iconStart="@tui.globe"
                (click)="onSetGuaWan(true)"
              >
                {{ 'Public' | i18n }}
              </button>
            </tui-data-list>
          </button>
        } @else {
          <tui-icon
            [icon]="address.access === 'public' ? '@tui.globe' : '@tui.house'"
          />
          <span>
            {{ (address.access === 'public' ? 'Public' : 'Local') | i18n }}
          </span>
        }
      </td>
      <td class="type">
        <span
          tuiBadge
          size="s"
          [appearance]="typeAppearance(address.hostnameInfo.metadata.kind)"
        >
          {{ address.type }}
        </span>
      </td>
      @if (showCert()) {
        <td class="cert-cell">
          <div class="cert">
            @if (address.certificate === 'Root CA') {
              <img src="assets/icons/favicon.svg" alt="" class="cert-icon" />
            } @else if (address.certificate.startsWith("Let's Encrypt")) {
              <img
                src="assets/icons/letsencrypt.svg"
                alt=""
                class="cert-icon"
              />
            } @else if (
              address.certificate !== '-' &&
              address.certificate !== 'Self signed'
            ) {
              <tui-icon icon="@tui.shield" class="cert-icon" />
            }
            {{ address.certificate }}
          </div>
        </td>
      }
      <td class="url-cell">
        <div class="url">
          @if (address.masked && currentlyMasked()) {
            <span>••••••••••••••••••••••••••••••••••••••••••••••••••••</span>
          } @else {
            <span [title]="address.url">
              @if (urlHtml(); as html) {
                <span [innerHTML]="html"></span>
              } @else {
                {{ address.url }}
              }
            </span>
          }
          @if (address.masked) {
            <button
              tuiIconButton
              appearance="flat-grayscale"
              size="xs"
              [iconStart]="currentlyMasked() ? '@tui.eye' : '@tui.eye-off'"
              (click)="currentlyMasked.set(!currentlyMasked())"
            >
              {{ (currentlyMasked() ? 'Reveal' : 'Hide') | i18n }}
            </button>
          }
        </div>
      </td>
      <td
        actions
        [address]="address"
        [packageId]="packageId()"
        [value]="value()"
        [disabled]="!isRunning()"
        [gatewayId]="gatewayId()"
        [(currentlyMasked)]="currentlyMasked"
        [style.width.rem]="5"
      ></td>
    }
  `,
  styles: `
    :host {
      grid-template-columns: fit-content(10rem) 1fr 2rem 2rem;
    }

    .access tui-icon {
      font-size: 1.3rem;
      margin-right: 0.7rem;
      vertical-align: middle;
    }

    .cert {
      display: flex;
      align-items: center;
      gap: 0.5rem;
    }

    .cert-icon {
      height: 1.25rem;
      width: 1.25rem;
      flex-shrink: 0;
    }

    tui-icon.cert-icon {
      font-size: 1.25rem;
    }

    .url {
      display: flex;
      align-items: center;
      gap: 0.25rem;

      span {
        white-space: normal;
        word-break: break-all;
        display: -webkit-box;
        -webkit-box-orient: vertical;
        -webkit-line-clamp: 1;
        overflow: hidden;
      }
    }

    :host-context(tui-root._mobile) {
      padding-inline-start: 0.75rem !important;
      row-gap: 0.25rem;

      .url button {
        display: none;
      }

      &::before {
        content: '';
        position: absolute;
        inset-inline-start: 0;
        top: 0.25rem;
        bottom: 0.25rem;
        width: 4px;
        background: var(--tui-status-positive);
        border-radius: 2px;
      }

      &._disabled::before {
        background: var(--tui-background-neutral-1-hover);
      }

      td {
        width: auto !important;
        align-content: center;
      }

      td:first-child {
        display: none;
      }

      .access {
        padding-right: 0;
        font: var(--tui-typography-body-m);
        font-weight: bold;

        tui-icon {
          display: none;
        }
      }

      .type {
        font: var(--tui-typography-body-m);
        font-weight: bold;
        color: var(--tui-text-primary);
        padding-inline: 0.5rem;
      }

      .cert-cell {
        grid-area: 2 / 1 / 2 / 3;

        .cert-icon {
          display: none;
        }
      }

      .url-cell {
        grid-area: 3 / 1 / 3 / 3;
      }

      // With no cert row, the URL takes its place directly under the header.
      &._no-cert .url-cell {
        grid-area: 2 / 1 / 2 / 3;
      }

      td:last-child {
        grid-area: 1 / 3 / 4 / 5;
        align-self: center;
        justify-self: end;
      }
    }
  `,
  imports: [
    i18nPipe,
    GatewayActionsComponent,
    TuiBadge,
    TuiButton,
    TuiChevron,
    TuiDataList,
    TuiDropdown,
    TuiIcon,
    TuiSwitch,
    FormsModule,
  ],
})
export class GatewayItemComponent {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly domainHealth = inject(DomainHealthService)

  readonly address = input.required<GatewayAddress>()
  readonly packageId = input('')
  readonly value = input<MappedServiceInterface | undefined>()
  readonly isRunning = input.required<boolean>()
  readonly gatewayId = input('')

  readonly toggling = signal(false)
  readonly currentlyMasked = signal(true)
  readonly guaOpen = signal(false)
  // The Certificate Authority column only shows when some address is SSL; kept
  // in sync with GatewayComponent's header (both derive from the same data).
  readonly showCert = computed(
    () =>
      this.value()?.gatewayGroups.some(g =>
        g.addresses.some(a => a.certificate !== '-'),
      ) ?? false,
  )
  readonly urlHtml = computed(() => {
    const { url, hostnameInfo } = this.address()
    const idx = url.indexOf(hostnameInfo.hostname)
    if (idx === -1) return null
    const prefix = url.slice(0, idx)
    const hostname = hostnameInfo.hostname
    const suffix = url.slice(idx + hostname.length)
    return `${prefix}<b>${hostname}</b>${suffix}`
  })

  typeAppearance(kind: string): string {
    switch (kind) {
      case 'public-domain':
      case 'private-domain':
        return 'info'
      case 'mdns':
        return 'positive'
      case 'ipv4':
        return 'warning'
      case 'ipv6':
        return 'neutral'
      default:
        return 'neutral'
    }
  }

  async onToggleEnabled() {
    const addr = this.address()
    const iface = this.value()
    if (!iface) return

    const enabled = !addr.enabled
    const params = {
      internalPort: iface.addressInfo.internalPort,
      address: addr.hostnameInfo,
      enabled,
      package: this.packageId(),
      host: iface.addressInfo.hostId,
    }

    this.toggling.set(true)
    await this.tasks.run(async () => {
      if (this.packageId()) {
        // A range spans >1 port and lives in a separate subtree, so it has its
        // own endpoint; a single-port binding is exactly 1.
        if (addr.count > 1) {
          await this.api.pkgBindingSetRangeAddressEnabled(params)
        } else {
          await this.api.pkgBindingSetAddressEnabled(params)
        }
      } else {
        await this.api.serverBindingSetAddressEnabled({
          internalPort: 80,
          address: addr.hostnameInfo,
          enabled,
        })
      }

      if (enabled) {
        const kind = addr.hostnameInfo.metadata.kind
        if (kind === 'public-domain' && addr.hostnameInfo.port !== null) {
          await this.domainHealth.checkPublicDomain(
            addr.hostnameInfo.hostname,
            this.gatewayId(),
            addr.hostnameInfo.port,
            addr.count,
          )
        } else if (kind === 'private-domain') {
          await this.domainHealth.checkPrivateDomain(
            this.gatewayId(),
            addr.hostnameInfo.hostname,
          )
        } else if (
          kind === 'ipv4' &&
          addr.access === 'public' &&
          addr.hostnameInfo.port !== null &&
          // A port range spans many ports; a single-port reachability check
          // would be misleading, so don't auto-test it on enable.
          addr.count === 1
        ) {
          await this.domainHealth.checkPortForward(
            this.gatewayId(),
            addr.hostnameInfo.port,
          )
        }
      }
    }, 'Saving')
    this.toggling.set(false)
  }

  async onSetGuaWan(wan: boolean) {
    const addr = this.address()
    const iface = this.value()
    // Selecting the already-active option shouldn't re-save.
    if (!iface || wan === (addr.access === 'public')) return

    this.toggling.set(true)
    await this.tasks.run(async () => {
      if (this.packageId()) {
        await this.api.pkgBindingSetGuaWan({
          internalPort: iface.addressInfo.internalPort,
          address: addr.hostnameInfo,
          wan,
          package: this.packageId(),
          host: iface.addressInfo.hostId,
        })
      } else {
        await this.api.serverBindingSetGuaWan({
          internalPort: 80,
          address: addr.hostnameInfo,
          wan,
        })
      }
    }, 'Saving')
    this.toggling.set(false)
  }
}
