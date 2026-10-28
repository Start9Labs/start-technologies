import { Component, inject, signal } from '@angular/core'
import { ErrorService, i18nPipe } from '@start9labs/shared'
import { utils } from '@start9labs/start-core'
import { TuiButton, TuiDialogContext, TuiIcon, TuiLoader } from '@taiga-ui/core'
import { TuiButtonLoading } from '@taiga-ui/kit'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DnsGateway } from './domain-validation.component'

export type PrivateDnsValidationData = {
  gateway: DnsGateway
  fqdn: string
  initialResults?: { configured: boolean }
}

@Component({
  selector: 'private-dns-validation',
  template: `
    @let gatewayName =
      context.data.gateway.name || context.data.gateway.ipInfo.name;
    @let internalIp = lanIp || ('Error' | i18n);

    <h2>{{ 'DNS Server Config' | i18n }}</h2>
    @if (isWireguard) {
      <p>
        {{ 'Gateway' | i18n }} "{{ gatewayName }}"
        {{ 'must be configured to use' | i18n }}
        {{ internalIp }}
        ({{ 'the address of this server on the tunnel' | i18n }})
        {{ 'as its DNS server' | i18n }}.
        {{
          'In StartTunnel, set the DNS for this subnet to this server.' | i18n
        }}
        {{ 'Or enable DNS Injection for this device on the gateway.' | i18n }}
      </p>
    } @else {
      <p>
        {{ 'Gateway' | i18n }} "{{ gatewayName }}"
        {{ 'must be configured to use' | i18n }}
        {{ internalIp }}
        ({{ 'the LAN IP address of this server' | i18n }})
        {{ 'as its DNS server' | i18n }}.
      </p>
    }

    <div class="desktop">
      <table [appTable]="[null, 'Gateway', 'DNS Server', null]">
        <tr>
          <td class="status">
            @if (loading()) {
              <tui-loader size="s" />
            } @else if (pass() === true) {
              <tui-icon class="g-positive" icon="@tui.check" />
            } @else if (pass() === false) {
              <tui-icon class="g-negative" icon="@tui.x" />
            } @else {
              <tui-icon class="g-secondary" icon="@tui.minus" />
            }
          </td>
          <td>{{ gatewayName }}</td>
          <td>{{ internalIp }}</td>
          <td>
            <button
              tuiButton
              size="s"
              [loading]="loading()"
              (click)="testDns()"
            >
              {{ 'Test' | i18n }}
            </button>
          </td>
        </tr>
      </table>
    </div>
    <div class="mobile">
      <div class="card">
        <div class="card-status">
          @if (loading()) {
            <tui-loader size="s" />
          } @else if (pass() === true) {
            <tui-icon class="g-positive" icon="@tui.check" />
          } @else if (pass() === false) {
            <tui-icon class="g-negative" icon="@tui.x" />
          } @else {
            <tui-icon class="g-secondary" icon="@tui.minus" />
          }
        </div>
        <div class="card-fields">
          <div class="field">
            <span class="field-label">{{ 'Gateway' | i18n }}</span>
            <span>{{ gatewayName }}</span>
          </div>
          <div class="field">
            <span class="field-label">{{ 'DNS Server' | i18n }}</span>
            <span>{{ internalIp }}</span>
          </div>
        </div>
        <button tuiButton size="s" [loading]="loading()" (click)="testDns()">
          {{ 'Test' | i18n }}
        </button>
      </div>
    </div>

    @if (!isManualMode) {
      <footer class="g-buttons padding-top">
        <button
          tuiButton
          appearance="flat"
          [disabled]="pass() === true"
          (click)="context.completeWith()"
        >
          {{ 'Later' | i18n }}
        </button>
        <button
          tuiButton
          [disabled]="pass() !== true"
          (click)="context.completeWith()"
        >
          {{ 'Done' | i18n }}
        </button>
      </footer>
    }
  `,
  styles: `
    h2 {
      margin: 2rem 0 0 0;
    }

    p {
      margin-top: 0.5rem;
    }

    tui-icon {
      font-size: 1rem;
      vertical-align: text-bottom;
    }

    .status {
      width: 3.2rem;
    }

    .padding-top {
      padding-top: 2rem;
    }

    td:last-child {
      text-align: end;
    }

    footer {
      margin-top: 1.5rem;
    }

    .mobile {
      display: none;
    }

    .card {
      display: flex;
      align-items: center;
      gap: 1rem;
      padding: 1rem;
      border: 1px solid var(--tui-border-normal);
      border-radius: var(--tui-radius-l);
      margin-top: 1rem;
    }

    .card-status {
      flex-shrink: 0;
      width: 1.5rem;
      text-align: center;
    }

    .card-fields {
      flex: 1;
      min-width: 0;
    }

    .field {
      display: flex;
      gap: 0.5rem;
    }

    .field-label {
      color: var(--tui-text-secondary);
      font: var(--tui-typography-body-s);

      &::after {
        content: ':';
      }
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
  imports: [
    TuiButton,
    i18nPipe,
    TableComponent,
    TuiButtonLoading,
    TuiIcon,
    TuiLoader,
  ],
})
export class PrivateDnsValidationComponent {
  private readonly errorService = inject(ErrorService)
  private readonly api = inject(ApiService)

  readonly context =
    injectContext<TuiDialogContext<void, PrivateDnsValidationData>>()

  get lanIp() {
    return this.context.data.gateway.ipInfo.subnets
      .map(s => utils.IpNet.parse(s))
      .find(s => s.isIpv4())?.address
  }

  get isWireguard(): boolean {
    return this.context.data.gateway.ipInfo.deviceType === 'wireguard'
  }

  readonly loading = signal(false)
  readonly pass = signal<boolean | undefined>(undefined)

  readonly isManualMode = !this.context.data.initialResults

  constructor() {
    const initial = this.context.data.initialResults
    if (initial) {
      this.pass.set(initial.configured)
    }
  }

  async testDns() {
    this.loading.set(true)

    try {
      const result = await this.api.rpc.net.gateway.checkDns({
        gateway: this.context.data.gateway.id,
        fqdn: this.context.data.fqdn,
      })

      this.pass.set(result)
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      this.loading.set(false)
    }
  }
}

export const PRIVATE_DNS_VALIDATION = new PolymorpheusComponent(
  PrivateDnsValidationComponent,
)
