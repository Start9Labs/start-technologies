import { Component, computed, effect, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { NonNullableFormBuilder, ReactiveFormsModule } from '@angular/forms'
import { WA_WINDOW } from '@ng-web-apis/common'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TuiNotificationService,
  tuiNumberFormatProvider,
  TuiTitle,
} from '@taiga-ui/core'
import { TUI_CONFIRM } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { firstValueFrom, startWith } from 'rxjs'
import { Footer } from 'src/app/components/footer'
import { Form } from 'src/app/components/form'
import { ApiService } from 'src/app/services/api/api.service'
import { ConnectionService } from 'src/app/services/connection.service'
import { provideFormService } from 'src/app/services/form.service'
import { LanIpv4Ip } from './form/ip'
import { LanIpv4Service } from './service'
import { LanIpv4Summary } from './summary'
import {
  buildRouterIp,
  getLanIpv4Form,
  getSecondOctetRange,
  isSecondOctetInRange,
  LanIpv4Form,
  resolveSecondOctet,
} from './utils'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    <header tuiHeader="h6">
      <h2 tuiTitle>{{ 'Summary' | i18n }}</h2>
    </header>
    <article lanIpv4Summary [formLoading]="!service.data()"></article>
    <header tuiHeader="h6">
      <h2 tuiTitle>{{ 'Settings' | i18n }}</h2>
    </header>
    <form
      [formGroup]="form"
      [formLoading]="!service.data()"
      (reset.prevent)="form.reset(service.data())"
      (ngSubmit)="onSave()"
    >
      <lan-ipv4-ip formGroupName="ip" />
      @if (service.data()) {
        <footer appFooter [blocked]="saveBlocked()"></footer>
      }
    </form>
  `,
  host: { class: 'g-page' },
  imports: [
    ReactiveFormsModule,
    TuiHeader,
    TuiTitle,
    Footer,
    Form,
    LanIpv4Summary,
    LanIpv4Ip,
    i18nPipe,
  ],
  providers: [
    provideFormService(LanIpv4Service),
    tuiNumberFormatProvider({ precision: 0 }),
  ],
})
export default class LanIpv4 {
  protected readonly builder = inject(NonNullableFormBuilder)
  protected readonly service = inject(LanIpv4Service)
  private readonly api = inject(ApiService)
  private readonly connection = inject(ConnectionService)
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly alerts = inject(TuiNotificationService)
  private readonly window = inject(WA_WINDOW)
  private readonly i18n = inject(i18nPipe)

  readonly form = getLanIpv4Form(this.builder)
  readonly hasStaticIps = signal(false)

  constructor() {
    effect(() => {
      const data = this.service.data()
      if (data && this.form.pristine) {
        this.form.reset(data)
      }
    })
    this.loadDependencies()
  }

  private readonly formValue = toSignal(
    this.form.valueChanges.pipe(startWith(this.form.getRawValue())),
    { requireSync: true },
  )

  readonly saveBlocked = computed(() => {
    const current = this.formValue()

    // Out-of-range second octet: block save and show the allowed RFC values.
    // Resolve first so the locked 192 block (whose control may hold a stale
    // value from a previous block) is never falsely flagged.
    const first = current.ip?.firstOctet
    const rawSecond = current.ip?.secondOctet
    const second =
      first != null && rawSecond != null
        ? resolveSecondOctet(first, rawSecond)
        : rawSecond
    if (
      first != null &&
      second != null &&
      !isSecondOctetInRange(first, second)
    ) {
      const { min, max } = getSecondOctetRange(first)
      return `Second octet must be ${min}–${max}`
    }

    if (!this.hasStaticIps()) return null
    const data = this.service.data()
    if (!data) return null
    // data.ip.secondOctet is backend-canonical (already resolved), so compare
    // against the resolved current value.
    if (
      first !== data.ip.firstOctet ||
      second !== data.ip.secondOctet ||
      current.ip?.routerOctet !== data.ip.routerOctet
    ) {
      return this.i18n.transform(
        'Cannot change subnet while devices have static IP reservations',
      )
    }
    return null
  })

  private async loadDependencies() {
    try {
      const devices = await this.api.devicesList()
      if (devices.some(d => d.ipv4_static)) {
        this.hasStaticIps.set(true)
      }
    } catch {
      // If we can't check, leave the form enabled — backend validates anyway
    }
  }

  async onSave() {
    if (this.form.invalid) {
      tuiMarkControlAsTouchedAndValidate(this.form)
      return
    }

    const oldIp = this.service.data()
      ? buildRouterIp(this.service.data()!.ip)
      : ''
    const newIp = buildRouterIp(this.form.getRawValue().ip)
    const currentHost = this.window.location.hostname

    if (oldIp !== newIp) {
      // IP changing — call API directly so VPN errors throw here (no toast)
      try {
        await this.service.saveForIpChange(this.form.getRawValue())
      } catch (e: any) {
        if (!e?.message?.includes('VPN client')) {
          this.alerts
            .open(e?.message || this.i18n.transform('Failed to save'), {
              appearance: 'negative',
            })
            .subscribe()
          return
        }
        const confirmed = await firstValueFrom(
          this.dialogs.open(TUI_CONFIRM, {
            label: this.i18n.transform('Inbound VPN Will Be Deleted'),
            data: {
              content: this.i18n.transform(
                'Changing the router IP will invalidate all existing VPN client configurations. The inbound VPN server and its peers will be removed and must be re-created.',
              ),
              yes: this.i18n.transform('Delete VPN & Continue'),
              no: this.i18n.transform('Cancel'),
            },
          }),
        ).catch(() => false)
        if (!confirmed) return
        try {
          await this.service.saveForIpChange(this.form.getRawValue(), true)
        } catch {
          return
        }
      }

      // The router's address is changing, so the current connection WILL drop:
      // the client must DHCP-renew into the new subnet before the new address is
      // routable. saveForIpChange() already suppressed the global indicator
      // (before the drop); hand off to it now to show the sticky "Reconnecting"
      // toast, poll the destination until it answers, and auto-redirect there.
      //
      // Preserve the current scheme/port. On a bare IP the old origin is gone
      // for good, so target the new IP directly (no DNS dependency; the cert is
      // reissued for it). On a hostname, reuse the same name — it re-resolves to
      // the new IP once DHCP renews.
      const { protocol, port } = this.window.location
      const suffix = port ? `:${port}` : ''
      const target =
        currentHost === oldIp
          ? `${protocol}//${newIp}${suffix}`
          : `${protocol}//${currentHost}${suffix}`
      this.connection.reconnectAt(
        target,
        this.i18n.transform('Reconnecting to the new address…'),
      )
      return
    }

    const saved = await this.service.save(this.form.getRawValue())
    if (saved) this.form.markAsPristine()
  }
}
