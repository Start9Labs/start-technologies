import { Component, computed, inject, signal } from '@angular/core'
import { ErrorService, i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiDialogContext } from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { formatPortRange } from 'src/app/utils/format-port-range'
import { DnsGateway } from './domain-validation.component'
import {
  PortCheckField,
  PortCheckTestComponent,
} from './port-check-test.component'
import { injectTestStatus } from './test-status'
import { TestStatusNoteComponent } from './test-status-note.component'

export type PortForwardValidationData = {
  gateway: DnsGateway
  port: number
  count: number
  packageId: string
  addSsl: boolean
  initialResults?: { portResult: T.CheckPortRes | null }
}

@Component({
  selector: 'port-forward-validation',
  template: `
    @let gatewayName =
      context.data.gateway.name || context.data.gateway.ipInfo.name;

    <h3 tuiHeader="h6">{{ 'Port Forwarding' | i18n }}</h3>
    <p>
      {{ 'In your gateway' | i18n }} "{{ gatewayName }}",
      {{ 'create this port forwarding rule' | i18n }}
    </p>

    <port-check-test
      [fields]="portFields"
      [testable]="!isRange"
      [result]="portResult()"
      [loading]="loading()"
      [disabled]="testDisabled()"
      (test)="testPort()"
    />

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
          [disabled]="portOk()"
          (click)="context.completeWith()"
        >
          {{ 'Later' | i18n }}
        </button>
        <button
          tuiButton
          [disabled]="!portOk()"
          (click)="context.completeWith()"
        >
          {{ 'Done' | i18n }}
        </button>
      </footer>
    }
  `,
  imports: [
    TuiButton,
    i18nPipe,
    TestStatusNoteComponent,
    PortCheckTestComponent,
    TuiHeader,
  ],
})
export class PortForwardValidationComponent {
  private readonly errorService = inject(ErrorService)
  private readonly api = inject(ApiService)

  readonly context =
    injectContext<TuiDialogContext<void, PortForwardValidationData>>()

  // Gates the port-forward test and drives the status shown while it is
  // unavailable.
  private readonly testStatus = injectTestStatus(
    this.context.data.packageId,
    this.context.data.addSsl,
  )
  protected readonly pkg = this.testStatus.pkg
  readonly testDisabled = this.testStatus.testDisabled

  // A port range forwards a span of ports and can't be tested a port at a time.
  readonly isRange = this.context.data.count > 1
  readonly portDisplay = formatPortRange(
    this.context.data.port,
    this.context.data.count,
  )

  readonly portFields: readonly PortCheckField[] = this.isRange
    ? [
        { label: 'External Range', value: this.portDisplay },
        { label: 'Internal Range', value: this.portDisplay },
      ]
    : [
        { label: 'External Port', value: this.portDisplay },
        { label: 'Internal Port', value: this.portDisplay },
      ]

  readonly loading = signal(false)
  readonly portResult = signal<T.CheckPortRes | undefined>(undefined)

  readonly portOk = computed(() => {
    const result = this.portResult()
    return !!result?.openExternally && !!result?.hairpinning
  })

  readonly isManualMode = !this.context.data.initialResults

  constructor() {
    const initial = this.context.data.initialResults
    if (initial) {
      if (initial.portResult) this.portResult.set(initial.portResult)
    }
  }

  async testPort() {
    this.loading.set(true)

    try {
      const result = await this.api.rpc.net.gateway.checkPort({
        gateway: this.context.data.gateway.id,
        port: this.context.data.port,
      })

      this.portResult.set(result)
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      this.loading.set(false)
    }
  }
}

export const PORT_FORWARD_VALIDATION = new PolymorpheusComponent(
  PortForwardValidationComponent,
)
