import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule, ReactiveFormsModule } from '@angular/forms'
import { RouterLink } from '@angular/router'
import { DocsLinkDirective, i18nPipe, TaskService } from '@start9labs/shared'
import { ISB } from '@start9labs/start-core'
import { TuiButton, TuiNotification, TuiTitle } from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'
import {
  catchError,
  combineLatest,
  defer,
  first,
  map,
  of,
  startWith,
  switchMap,
} from 'rxjs'
import { FormGroupComponent } from 'src/app/routes/portal/components/form/containers/group.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { FormService } from 'src/app/services/form.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { TitleDirective } from 'src/app/services/title.service'
import { configBuilderToSpec } from 'src/app/utils/configBuilderToSpec'

// IPv4
const ipv4 =
  /(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)(\.(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)){3}/

// IPv6 (your existing pattern)
const ipv6 =
  /(([0-9a-fA-F]{1,4}:){7,7}[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,7}:|([0-9a-fA-F]{1,4}:){1,6}:[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,5}(:[0-9a-fA-F]{1,4}){1,2}|([0-9a-fA-F]{1,4}:){1,4}(:[0-9a-fA-F]{1,4}){1,3}|([0-9a-fA-F]{1,4}:){1,3}(:[0-9a-fA-F]{1,4}){1,4}|([0-9a-fA-F]{1,4}:){1,2}(:[0-9a-fA-F]{1,4}){1,5}|[0-9a-fA-F]{1,4}:((:[0-9a-fA-F]{1,4}){1,6})|:((:[0-9a-fA-F]{1,4}){1,7}|:)|fe80:(:[0-9a-fA-F]{0,4}){0,4}%[0-9a-zA-Z]{1,}|::(ffff(:0{1,4}){0,1}:){0,1}((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])|([0-9a-fA-F]{1,4}:){1,4}:((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9]))/

@Component({
  template: `
    <ng-container *title>
      <div>
        <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
          {{ 'Back' | i18n }}
        </a>
        {{ 'DNS Servers' | i18n }}
        <a
          tuiIconButton
          size="xs"
          docsLink
          path="/start-os/dns.html"
          appearance="icon"
          iconStart="@tui.book-open-text"
        ></a>
      </div>
    </ng-container>
    @if (data(); as d) {
      <form [formGroup]="d.form">
        <header tuiHeader="body-l">
          <h3 tuiTitle>
            <b>
              {{ 'DNS Servers' | i18n }}
              <a
                tuiIconButton
                size="xs"
                docsLink
                path="/start-os/dns.html"
                appearance="icon"
                iconStart="@tui.book-open-text"
              >
                {{ 'Documentation' | i18n }}
              </a>
            </b>
          </h3>
        </header>

        <form-group [spec]="d.spec" />

        @if (resolves() === false) {
          <p tuiNotification appearance="negative">
            {{
              'StartOS cannot resolve domain names with the current DNS servers. Services that reach the internet by name will fail.'
                | i18n
            }}
            @if (d.dhcp) {
              {{
                'Your router is not providing a DNS server that answers. Select "Static" above and enter one.'
                  | i18n
              }}
            }
          </p>
        }

        @for (warn of d.warn; track $index) {
          <p>{{ warn }}</p>
        }

        <footer>
          <button
            tuiButton
            size="l"
            [disabled]="d.form.invalid || d.form.pristine"
            (click)="save(d.form.value)"
          >
            {{ 'Save' | i18n }}
          </button>
        </footer>
      </form>
    }
  `,
  styles: `
    :host {
      max-width: 36rem;
    }

    :host-context(tui-root._mobile) [tuiHeader] {
      display: none;
    }

    form header,
    form footer {
      margin: 1rem 0;
      display: flex;
      gap: 1rem;
    }

    footer {
      justify-content: flex-end;
    }
  `,
  imports: [
    FormsModule,
    ReactiveFormsModule,
    FormGroupComponent,
    TuiButton,
    TuiHeader,
    TuiNotification,
    TuiTitle,
    RouterLink,
    TitleDirective,
    i18nPipe,
    DocsLinkDirective,
  ],
})
export default class SystemDnsComponent {
  private readonly tasks = inject(TaskService)
  private readonly formService = inject(FormService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)

  private readonly dnsSpec = ISB.InputSpec.of({
    strategy: ISB.Value.union({
      name: 'strategy',
      default: 'dhcp',
      description: `<ul><li><b>DHCP</b>: ${this.i18n.transform('Use the DNS servers provided by your router')}</li><li><b>${this.i18n.transform('Static')}</b>: ${this.i18n.transform('Use DNS servers you specify manually')}</li></ul>`,
      variants: ISB.Variants.of({
        dhcp: {
          name: 'DHCP',
          spec: ISB.InputSpec.of({}),
        },
        static: {
          name: this.i18n.transform('Static'),
          spec: ISB.InputSpec.of({
            servers: ISB.Value.list(
              ISB.List.text(
                {
                  name: this.i18n.transform('Servers'),
                  minLength: 1,
                  maxLength: 3,
                },
                {
                  placeholder: '1.1.1.1',
                  patterns: [
                    {
                      regex: `^(${ipv4.source}(:\\d{1,5})?|${ipv6.source}|\\[${ipv6.source}\\](:\\d{1,5})?)$`,
                      description: this.i18n.transform(
                        'Must be a valid IPv4 or Ipv6 address with optional port',
                      ),
                    },
                  ],
                },
              ),
            ),
          }),
        },
      }),
    }),
  })

  // Configured servers can be present and simply not answer — indistinguishable
  // from a working setup until something asks — so this resolves a name rather
  // than inspecting the list. Re-runs when the servers change, so saving a fix
  // clears the warning. `null` while in flight: absence of an answer yet is not
  // a failure.
  protected readonly resolves = toSignal(
    this.patch.watch$('serverInfo', 'network', 'dns').pipe(
      switchMap(() =>
        defer(() => this.api.queryDns({ fqdn: 'registry.start9.com' })).pipe(
          map(res => !!res.ipv4 || !!res.ipv6),
          catchError(() => of(false)),
          startWith(null),
        ),
      ),
    ),
  )

  readonly data = toSignal(
    combineLatest([
      this.patch.watch$('packageData').pipe(first()),
      this.patch.watch$('serverInfo', 'network'),
    ]).pipe(
      switchMap(async ([pkgs, { gateways, dns }]) => {
        const spec = await configBuilderToSpec(this.dnsSpec)

        const dhcpServers = { servers: dns.dhcpServers.join(', ') }

        const current: (typeof this.dnsSpec._TYPE)['strategy'] =
          dns.staticServers
            ? {
                selection: 'static',
                value: { servers: dns.staticServers || [] },
                other: {
                  dhcp: dhcpServers,
                },
              }
            : {
                selection: 'dhcp',
                value: dhcpServers,
              }

        const form = this.formService.createForm(spec, { strategy: current })

        const warn = Object.values(pkgs).some(p =>
          Object.values(p.hosts).some(
            h => Object.keys(h?.privateDomains || {}).length,
          ),
        )
          ? Object.values(gateways)
              .filter(g =>
                (dns.staticServers || dns.dhcpServers).some(d =>
                  g.ipInfo?.lanIp.includes(d),
                ),
              )
              .map(
                g =>
                  `${this.i18n.transform('Warning. StartOS is currently using the following gateway for DNS')}: ${g.ipInfo!.name}. ${this.i18n.transform('If you intend to use this gateway for private domain resolution, set alternative static DNS servers using the form above.')}`,
              )
          : []

        return {
          spec,
          form,
          warn,
          dhcp: !dns.staticServers,
        }
      }),
    ),
  )

  async save({ strategy }: typeof this.dnsSpec._TYPE): Promise<void> {
    this.tasks.run(
      async () =>
        await this.api.setDns({
          servers:
            strategy.selection === 'dhcp' ? null : strategy.value.servers,
        }),
      'Saving',
    )
  }
}
