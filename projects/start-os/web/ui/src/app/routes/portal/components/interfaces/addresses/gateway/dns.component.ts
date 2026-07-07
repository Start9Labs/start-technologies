import { Component, computed, inject, signal } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { ErrorService, i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiDialogContext } from '@taiga-ui/core'
import { TuiButtonLoading, TuiSwitch } from '@taiga-ui/kit'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { CheckIconComponent } from 'src/app/routes/portal/components/check-icon.component'
import { PortCheckIconComponent } from 'src/app/routes/portal/components/port-check-icon.component'
import { PortCheckWarningsComponent } from 'src/app/routes/portal/components/port-check-warnings.component'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { formatPortRange } from 'src/app/utils/format-port-range'
import { dnsAllPass, getGua, getLanIpv4, portAllPass } from 'src/app/utils/gua'
import { parse } from 'tldts'

export type DnsGateway = T.NetworkInterfaceInfo & {
  id: string
  ipInfo: T.IpInfo
}

export type DomainValidationData = {
  fqdn: string
  gateway: DnsGateway
  port: number
  count: number
  initialResults?: {
    dns: T.QueryDnsRes | null
    portResult: T.CheckPortRes | null
    portV6Result: T.CheckPortV6Res | null
  }
}

@Component({
  selector: 'domain-validation',
  template: `
    @let wanIp = context.data.gateway.ipInfo.wanIp || ('Error' | i18n);
    @let gatewayName =
      context.data.gateway.name || context.data.gateway.ipInfo.name;

    <h2>{{ 'DNS' | i18n }}</h2>
    <p>
      {{ 'In your domain registrar for' | i18n }} {{ domain }},
      {{ (gua ? 'create these DNS records' : 'create this DNS record') | i18n }}
    </p>

    @if (context.data.gateway.ipInfo.deviceType !== 'wireguard') {
      <label>
        IP
        <input
          type="checkbox"
          tuiSwitch
          [(ngModel)]="ddns"
          (ngModelChange)="dnsResult.set(undefined)"
        />
        {{ 'Dynamic DNS' | i18n }}
      </label>
    }

    <div class="desktop">
      <table [appTable]="[null, 'Type', 'Host', 'Value', null]">
        <tr>
          <td class="status">
            <check-icon [pass]="dnsV4Pass()" [loading]="dnsLoading()" />
          </td>
          <td>{{ ddns ? 'ALIAS' : 'A' }}</td>
          <td>*</td>
          <td>{{ ddns ? '[DDNS Address]' : wanIp }}</td>
          <td>
            <button
              tuiButton
              size="s"
              [loading]="dnsLoading()"
              (click)="testDns()"
            >
              {{ 'Test' | i18n }}
            </button>
          </td>
        </tr>
        @if (gua) {
          <tr>
            <td class="status">
              <check-icon [pass]="dnsV6Pass()" [loading]="dnsLoading()" />
            </td>
            <td>AAAA</td>
            <td>*</td>
            <td>{{ gua }}</td>
            <td></td>
          </tr>
        }
      </table>
    </div>
    <div class="mobile">
      <div class="card">
        <div class="card-status">
          <check-icon [pass]="dnsV4Pass()" [loading]="dnsLoading()" />
        </div>
        <div class="card-fields">
          <div class="field">
            <span class="field-label">{{ 'Type' | i18n }}</span>
            <span>{{ ddns ? 'ALIAS' : 'A' }}</span>
          </div>
          <div class="field">
            <span class="field-label">{{ 'Host' | i18n }}</span>
            <span>*</span>
          </div>
          <div class="field">
            <span class="field-label">{{ 'Value' | i18n }}</span>
            <span>{{ ddns ? '[DDNS Address]' : wanIp }}</span>
          </div>
        </div>
        <button tuiButton size="s" [loading]="dnsLoading()" (click)="testDns()">
          {{ 'Test' | i18n }}
        </button>
      </div>
      @if (gua) {
        <div class="card">
          <div class="card-status">
            <check-icon [pass]="dnsV6Pass()" [loading]="dnsLoading()" />
          </div>
          <div class="card-fields">
            <div class="field">
              <span class="field-label">{{ 'Type' | i18n }}</span>
              <span>AAAA</span>
            </div>
            <div class="field">
              <span class="field-label">{{ 'Host' | i18n }}</span>
              <span>*</span>
            </div>
            <div class="field">
              <span class="field-label">{{ 'Value' | i18n }}</span>
              <span>{{ gua }}</span>
            </div>
          </div>
        </div>
      }
    </div>

    <h2>{{ 'Port Forwarding' | i18n }}</h2>
    <p>
      {{ 'In your gateway' | i18n }} "{{ gatewayName }}",
      {{ 'create this port forwarding rule' | i18n }}
    </p>
    <p>
      {{
        (isRange
          ? 'Or enable automatic port forwarding (PCP) on the gateway. UPnP and NAT-PMP do not support port ranges.'
          : 'Or enable automatic port forwarding (UPnP / NAT-PMP / PCP) on the gateway.'
        ) | i18n
      }}
    </p>

    @let portRes = portResult();

    <div class="desktop">
      <table
        [class.range-table]="isRange"
        [appTable]="
          isRange
            ? ['External Range', 'Internal Range']
            : [null, 'External', 'Internal', null]
        "
      >
        <tr>
          @if (!isRange) {
            <td class="status">
              <port-check-icon [result]="portRes" [loading]="portLoading()" />
            </td>
          }
          <td>{{ externalAddr }}</td>
          <td>{{ internalAddr }}</td>
          @if (!isRange) {
            <td>
              <button
                tuiButton
                size="s"
                [loading]="portLoading()"
                (click)="testPort()"
              >
                {{ 'Test' | i18n }}
              </button>
            </td>
          }
        </tr>
      </table>
    </div>
    <div class="mobile">
      <div class="card">
        @if (!isRange) {
          <div class="card-status">
            <port-check-icon [result]="portRes" [loading]="portLoading()" />
          </div>
        }
        <div class="card-fields">
          <div class="field">
            <span class="field-label">
              {{ (isRange ? 'External Range' : 'External') | i18n }}
            </span>
            <span>{{ externalAddr }}</span>
          </div>
          <div class="field">
            <span class="field-label">
              {{ (isRange ? 'Internal Range' : 'Internal') | i18n }}
            </span>
            <span>{{ internalAddr }}</span>
          </div>
        </div>
        @if (!isRange) {
          <button
            tuiButton
            size="s"
            [loading]="portLoading()"
            (click)="testPort()"
          >
            {{ 'Test' | i18n }}
          </button>
        }
      </div>
    </div>

    <port-check-warnings [result]="portRes" />

    @if (!isRange && gua) {
      <h2>{{ 'IPv6 Firewall' | i18n }}</h2>
      <p>
        {{
          'IPv6 has no port forwarding — your server is reachable directly at its global address. Your gateway firewall must allow inbound connections to it, or enable automatic firewall configuration (PCP) on the gateway.'
            | i18n
        }}
      </p>

      <div class="desktop">
        <table [appTable]="[null, 'Address', null]">
          <tr>
            <td class="status">
              <port-check-icon
                [result]="portV6Result() || undefined"
                [loading]="portV6Loading()"
              />
            </td>
            <td>{{ ipv6Addr }}</td>
            <td>
              <button
                tuiButton
                size="s"
                [loading]="portV6Loading()"
                (click)="testPortV6()"
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
            <port-check-icon
              [result]="portV6Result() || undefined"
              [loading]="portV6Loading()"
            />
          </div>
          <div class="card-fields">
            <div class="field">
              <span class="field-label">{{ 'Address' | i18n }}</span>
              <span>{{ ipv6Addr }}</span>
            </div>
          </div>
          <button
            tuiButton
            size="s"
            [loading]="portV6Loading()"
            (click)="testPortV6()"
          >
            {{ 'Test' | i18n }}
          </button>
        </div>
      </div>
    }

    @if (!isManualMode) {
      <footer class="g-buttons padding-top">
        <button
          tuiButton
          appearance="flat"
          [disabled]="allPass()"
          (click)="context.completeWith()"
        >
          {{ 'Later' | i18n }}
        </button>
        <button
          tuiButton
          [disabled]="!allPass()"
          (click)="context.completeWith()"
        >
          {{ 'Done' | i18n }}
        </button>
      </footer>
    }
  `,
  styles: `
    label {
      display: flex;
      gap: 0.75rem;
      align-items: center;
      margin: 1rem 0;
    }

    h2 {
      margin: 2rem 0 0 0;
    }

    p {
      margin-top: 0.5rem;
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

    // The port-forwarding range table has no status/Test columns, so its last
    // cell is the internal-range value — keep it left-aligned with its header.
    .range-table td:last-child {
      text-align: start;
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
    TuiSwitch,
    FormsModule,
    TuiButtonLoading,
    CheckIconComponent,
    PortCheckIconComponent,
    PortCheckWarningsComponent,
  ],
})
export class DomainValidationComponent {
  private readonly errorService = inject(ErrorService)
  private readonly api = inject(ApiService)

  ddns = false

  readonly context =
    injectContext<TuiDialogContext<void, DomainValidationData>>()

  readonly domain =
    parse(this.context.data.fqdn).domain || this.context.data.fqdn

  private readonly wanIp = this.context.data.gateway.ipInfo.wanIp
  private readonly lanIpv4 = getLanIpv4(this.context.data.gateway.ipInfo)
  // The gateway's IPv6 GUA (the AAAA target), if it has one. When present the
  // domain is DualStack and the modal verifies both families.
  readonly gua = getGua(this.context.data.gateway.ipInfo)

  // A port range forwards a span of ports and can't be tested a port at a time;
  // only its DNS is verifiable here.
  readonly isRange = this.context.data.count > 1
  readonly portDisplay = formatPortRange(
    this.context.data.port,
    this.context.data.count,
  )

  // Full socket addresses make the boxes self-distinguishing: the IPv4 forward
  // (external WAN -> internal LAN) vs the IPv6 firewall (the server's own GUA).
  readonly externalAddr = this.socketAddr(this.wanIp)
  readonly internalAddr = this.socketAddr(this.lanIpv4)
  readonly ipv6Addr = this.gua ? `[${this.gua}]:${this.portDisplay}` : ''

  readonly dnsLoading = signal(false)
  readonly portLoading = signal(false)
  readonly portV6Loading = signal(false)
  readonly dnsResult = signal<T.QueryDnsRes | undefined>(undefined)
  readonly portResult = signal<T.CheckPortRes | undefined>(undefined)
  readonly portV6Result = signal<T.CheckPortV6Res | undefined>(undefined)

  readonly dnsV4Pass = computed(() => {
    const dns = this.dnsResult()
    return dns ? dns.ipv4 === this.wanIp : undefined
  })
  readonly dnsV6Pass = computed(() => {
    const dns = this.dnsResult()
    return dns && this.gua ? dns.ipv6 === this.gua : undefined
  })

  readonly allPass = computed(
    () =>
      dnsAllPass(this.dnsResult(), this.wanIp, this.gua) &&
      (this.isRange ||
        portAllPass(this.portResult(), this.portV6Result(), this.gua)),
  )

  readonly isManualMode = !this.context.data.initialResults

  constructor() {
    const initial = this.context.data.initialResults
    if (initial) {
      if (initial.dns) this.dnsResult.set(initial.dns)
      if (initial.portResult) this.portResult.set(initial.portResult)
      if (initial.portV6Result) this.portV6Result.set(initial.portV6Result)
    }
  }

  async testDns() {
    this.dnsLoading.set(true)

    try {
      this.dnsResult.set(
        await this.api.queryDns({ fqdn: this.context.data.fqdn }),
      )
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      this.dnsLoading.set(false)
    }
  }

  async testPort() {
    this.portLoading.set(true)

    try {
      this.portResult.set(
        await this.api.checkPort({
          gateway: this.context.data.gateway.id,
          port: this.context.data.port,
        }),
      )
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      this.portLoading.set(false)
    }
  }

  // A separate endpoint from checkPort so the IPv4 and IPv6 reachability probes
  // run independently — clicking one Test does not trigger the other family.
  async testPortV6() {
    this.portV6Loading.set(true)

    try {
      this.portV6Result.set(
        (await this.api.checkPortV6({
          gateway: this.context.data.gateway.id,
          port: this.context.data.port,
        })) ?? undefined,
      )
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      this.portV6Loading.set(false)
    }
  }

  private socketAddr(ip: string | null): string {
    return ip ? `${ip}:${this.portDisplay}` : this.portDisplay
  }
}

export const DOMAIN_VALIDATION = new PolymorpheusComponent(
  DomainValidationComponent,
)
