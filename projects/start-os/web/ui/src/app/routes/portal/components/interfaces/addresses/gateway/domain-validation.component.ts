import { Component, computed, inject, signal } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { ErrorService, i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiDialogContext, TuiLabel } from '@taiga-ui/core'
import {
  TuiButtonLoading,
  TuiSwitch,
  tuiSwitchOptionsProvider,
} from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { CheckIconComponent } from 'src/app/routes/portal/components/check-icon.component'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { formatPortRange } from 'src/app/utils/format-port-range'
import { dnsAllPass, getGua, portAllPass } from 'src/app/utils/gua'
import { parse } from 'tldts'
import {
  PortCheckField,
  PortCheckTestComponent,
} from './port-check-test.component'
import { injectTestStatus } from './test-status'
import { TestStatusNoteComponent } from './test-status-note.component'

export type DnsGateway = T.NetworkInterfaceInfo & {
  id: string
  ipInfo: T.IpInfo
}

export type DomainValidationData = {
  fqdn: string
  gateway: DnsGateway
  port: number
  count: number
  packageId: string
  addSsl: boolean
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

    <h3 tuiHeader="h6">{{ 'DNS' | i18n }}</h3>
    <p>
      {{ 'In your domain registrar for' | i18n }} {{ domain }},
      {{ (gua ? 'create these DNS records' : 'create this DNS record') | i18n }}
    </p>

    @if (context.data.gateway.ipInfo.deviceType !== 'wireguard') {
      <p>
        <label tuiLabel>
          IP
          <input
            type="checkbox"
            tuiSwitch
            [style.margin-inline.rem]="0.5"
            [showIcons]="false"
            [(ngModel)]="ddns"
            (ngModelChange)="dnsResult.set(undefined)"
          />
          {{ 'Dynamic DNS' | i18n }}
        </label>
      </p>
    }

    <table [appTable]="[null, 'Type', 'Host', 'Value', null]">
      <tr>
        <td>
          <check-icon [pass]="dnsV4Pass()" [loading]="dnsLoading()" />
        </td>
        <td [attr.data-label]="'Type' | i18n">{{ ddns ? 'ALIAS' : 'A' }}</td>
        <td [attr.data-label]="'Host' | i18n">*</td>
        <td [attr.data-label]="'Value' | i18n">
          {{ ddns ? '[DDNS Address]' : wanIp }}
        </td>
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
          <td>
            <check-icon [pass]="dnsV6Pass()" [loading]="dnsLoading()" />
          </td>
          <td [attr.data-label]="'Type' | i18n">AAAA</td>
          <td [attr.data-label]="'Host' | i18n">*</td>
          <td [attr.data-label]="'Value' | i18n">{{ gua }}</td>
          <td></td>
        </tr>
      }
    </table>

    <h3 tuiHeader="h6">{{ 'Port Forwarding' | i18n }}</h3>
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

    <port-check-test
      [fields]="portFields"
      [testable]="!isRange"
      [result]="portResult()"
      [loading]="portLoading()"
      [disabled]="testDisabled()"
      (test)="testPort()"
    />

    @if (!isRange && gua) {
      <h3 tuiHeader="h6">{{ 'IPv6 Firewall' | i18n }}</h3>
      <p>
        {{
          'IPv6 has no port forwarding — your server is reachable directly at its global address. Your gateway firewall must allow inbound connections to it, or enable automatic firewall configuration (PCP) on the gateway.'
            | i18n
        }}
      </p>

      <port-check-test
        [fields]="firewallFields"
        [result]="portV6Result()"
        [loading]="portV6Loading()"
        [disabled]="testDisabled()"
        (test)="testPortV6()"
      />
    }

    @if (!isRange && testDisabled()) {
      @if (pkg(); as p) {
        <test-status-note [pkg]="p" />
      }
    }

    @if (!isManualMode) {
      <footer class="g-buttons">
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
    table {
      margin-block-end: 2rem;
    }

    tr {
      grid-template-columns: min-content 1fr min-content;
      margin-inline: 1rem;
    }

    td:first-child {
      inline-size: 0;
      min-inline-size: fit-content;
      place-self: center;
      grid-column: 1;
      grid-row: 1 / span 3;
      margin-inline-end: 1rem;
    }

    td:last-child {
      text-align: end;
      padding-inline: 0.5rem;
      grid-column: 3;
      grid-row: 1 / span 3;
      place-self: center;
    }

    :host-context(tui-root._mobile) {
      table {
        color: var(--tui-text-primary);
        border-radius: var(--tui-radius-l);
        box-shadow: inset 0 0 0 1px var(--tui-border-normal);
      }

      td[data-label] {
        grid-column: 2;

        &::before {
          content: attr(data-label) ': ';
          color: var(--tui-text-secondary);
        }
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
    TestStatusNoteComponent,
    PortCheckTestComponent,
    TuiHeader,
    TuiLabel,
  ],
  providers: [tuiSwitchOptionsProvider({ appearance: () => 'primary' })],
})
export class DomainValidationComponent {
  private readonly errorService = inject(ErrorService)
  private readonly api = inject(ApiService)

  ddns = false

  readonly context =
    injectContext<TuiDialogContext<void, DomainValidationData>>()

  // Gates the port-forward / firewall tests and drives the status shown while
  // they are unavailable. DNS resolution does not depend on the service, so its
  // Test always stays on.
  private readonly testStatus = injectTestStatus(
    this.context.data.packageId,
    this.context.data.addSsl,
  )
  protected readonly pkg = this.testStatus.pkg
  readonly testDisabled = this.testStatus.testDisabled

  readonly domain =
    parse(this.context.data.fqdn).domain || this.context.data.fqdn

  private readonly wanIp = this.context.data.gateway.ipInfo.wanIp
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

  readonly ipv6Addr = this.gua ? `[${this.gua}]:${this.portDisplay}` : ''

  readonly portFields: readonly PortCheckField[] = this.isRange
    ? [
        { label: 'External Range', value: this.portDisplay },
        { label: 'Internal Range', value: this.portDisplay },
      ]
    : [
        { label: 'External Port', value: this.portDisplay },
        { label: 'Internal Port', value: this.portDisplay },
      ]
  readonly firewallFields: readonly PortCheckField[] = [
    { label: 'Address', value: this.ipv6Addr },
  ]

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
}

export const DOMAIN_VALIDATION = new PolymorpheusComponent(
  DomainValidationComponent,
)
