import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { ReactiveFormsModule, UntypedFormGroup } from '@angular/forms'
import { DocsLinkDirective, i18nPipe, TaskService } from '@start9labs/shared'
import { ISB } from '@start9labs/start-core'
import {
  tuiMarkControlAsTouchedAndValidate,
  TuiValueChanges,
} from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiDialogContext,
  TuiNotification,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiConfirmService } from '@taiga-ui/kit'
import { TuiForm, TuiHeader } from '@taiga-ui/layout'
import { injectContext } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'
import { combineLatest, first, switchMap } from 'rxjs'

import { FormGroupComponent } from 'src/app/routes/portal/components/form/containers/group.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { FormService, trim } from 'src/app/services/form.service'
import {
  gatewayName,
  isConnectedGateway,
} from 'src/app/services/gateway.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { configBuilderToSpec } from 'src/app/utils/configBuilderToSpec'
import {
  dnsSelectionServers,
  dnsSelectionSpec,
  dnsSelectionValue,
} from 'src/app/utils/dns-spec'

@Component({
  template: `
    <header tuiHeader>
      <h2 tuiTitle [id]="context.id">
        {{ 'Edit' | i18n }}
        <span tuiSubtitle>{{ data()?.name }} ({{ context.data }})</span>
      </h2>
    </header>
    @if (data(); as d) {
      @if (d.gateway; as gateway) {
        <form
          [formGroup]="d.form"
          (tuiValueChanges)="confirm.markAsDirty()"
          (submit.prevent)="save(d.form)"
        >
          <form-group [spec]="d.spec" />
          @if (gateway.dns.source === 'public-fallback') {
            <p tuiNotification appearance="warning" role="status">
              {{
                'This gateway uses Cloudflare public DNS because it provides no usable DNS servers. Queries travel through this gateway.'
                  | i18n
              }}
            </p>
          }
          @for (warn of d.warn; track $index) {
            <p tuiNotification appearance="warning">{{ warn }}</p>
          }
          <div tuiForm="m">
            <footer>
              <a
                tuiButton
                appearance="flat-grayscale"
                docsLink
                path="/start-os/dns.html"
              >
                {{ 'Documentation' | i18n }}
              </a>
              <button tuiButton [disabled]="d.form.pristine" type="submit">
                {{ 'Save' | i18n }}
              </button>
            </footer>
          </div>
        </form>
      } @else {
        <p tuiNotification appearance="warning">
          {{ 'Gateway is no longer available' | i18n }}
        </p>
      }
    }
  `,
  imports: [
    ReactiveFormsModule,
    DocsLinkDirective,
    FormGroupComponent,
    i18nPipe,
    TuiButton,
    TuiForm,
    TuiHeader,
    TuiNotification,
    TuiTitle,
    TuiValueChanges,
  ],
})
export default class EditGateway {
  private readonly tasks = inject(TaskService)
  private readonly formService = inject(FormService)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly api = inject(ApiService)
  private readonly i18n = inject(i18nPipe)
  private readonly inputSpec = ISB.InputSpec.of({
    name: ISB.Value.text({
      name: this.i18n.transform('Name'),
      required: true,
      default: '',
    }),
    dns: dnsSelectionSpec(this.i18n, 'Auto'),
  })
  private readonly spec = configBuilderToSpec(this.inputSpec)
  private currentForm?: { key: string; name: string; form: UntypedFormGroup }

  protected readonly confirm = inject(TuiConfirmService)
  protected readonly context = injectContext<TuiDialogContext<void, string>>()
  protected readonly data = toSignal(
    combineLatest([
      this.patch.watch$('packageData').pipe(first()),
      this.patch.watch$('serverInfo', 'network'),
    ]).pipe(
      switchMap(async ([pkgs, { gateways, host }]) => {
        const gateway = gateways[this.context.data]
        const name = gatewayName(this.context.data, gateway)
        const spec = await this.spec
        const key = JSON.stringify([
          this.context.data,
          gateway?.name,
          gateway?.dns.staticServers,
        ])
        const form =
          this.currentForm?.key === key
            ? this.currentForm.form
            : this.formService.createForm(spec, {
                name,
                dns: dnsSelectionValue(gateway?.dns.staticServers ?? null),
              })
        if (this.currentForm?.form !== form) {
          this.confirm.markAsPristine()
          this.currentForm = { key, name, form }
        }
        const warn: string[] = []
        if (
          gateway &&
          (Object.keys(host.privateDomains).length > 0 ||
            Object.values(pkgs).some(p =>
              Object.values(p.hosts).some(
                h => Object.keys(h?.privateDomains || {}).length,
              ),
            ))
        ) {
          for (const [id, g] of Object.entries(gateways)) {
            if (
              gateway.dns.servers.some(server =>
                g.ipInfo?.lanIp.some(
                  ip =>
                    server === ip ||
                    (!ip.includes(':') && server.startsWith(ip + ':')) ||
                    server.startsWith('[' + ip + ']:'),
                ),
              )
            ) {
              warn.push(
                `${this.i18n.transform('Warning. StartOS is currently using the following gateway for DNS')}: ${gatewayName(id, g)}. ${this.i18n.transform('If you intend to use this gateway for private domain resolution, set alternative static DNS servers using the form above.')}`,
              )
            }
          }
        }
        return {
          gateway: isConnectedGateway(gateway) ? gateway : undefined,
          name,
          spec,
          form,
          warn,
        }
      }),
    ),
  )

  protected async save(form: UntypedFormGroup): Promise<void> {
    if (!this.data()?.gateway) return
    trim(form)
    tuiMarkControlAsTouchedAndValidate(form)
    if (form.invalid) return
    const { name, dns } = form.value as typeof this.inputSpec._TYPE
    if (
      await this.tasks.run(
        async () =>
          await this.api.rpc.net.gateway.edit({
            id: this.context.data,
            name: name === this.currentForm?.name ? null : name,
            dns: { servers: dnsSelectionServers(dns) },
          }),
        'Saving',
      )
    ) {
      this.confirm.markAsPristine()
      this.context.completeWith()
    }
  }
}
