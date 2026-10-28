import { Component, computed, inject } from '@angular/core'
import { TuiBadge, TuiStatus } from '@taiga-ui/kit'
import { Summary } from 'src/app/components/summary'
import { buildChain } from 'src/app/routes/outbound/utils'
import OutboundVPN from 'src/app/routes/outbound/routes/vpn/index'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { i18nKey } from 'src/app/i18n/i18n.providers'
import { i18nService } from 'src/app/i18n/i18n.service'
import { OutboundVpn } from 'src/app/services/api/api.service'
import { VPNChain } from './chain'

const STATUS: Record<
  OutboundVpn['status'],
  { appearance: string; label: i18nKey; hint: i18nKey }
> = {
  connected: {
    appearance: 'positive',
    label: 'Connected',
    hint: 'The tunnel completed a handshake recently. Traffic from the profiles using this VPN goes through it.',
  },
  idle: {
    appearance: 'neutral',
    label: 'Idle',
    hint: "Enabled, with no recent traffic. Nothing has needed this tunnel, so it isn't connected right now. It connects when a profile using it sends traffic.",
  },
  'not-responding': {
    appearance: 'negative',
    label: 'Not responding',
    hint: "The router is sending to your VPN provider's server and getting no reply. Profiles using this VPN have no internet until it responds. Check with your provider that the server is online and that this configuration is still valid. You may need to download a new one. To get these profiles back online in the meantime, switch them to another VPN. Disabling this VPN also restores their internet, but their traffic will then bypass the VPN.",
  },
  disabled: {
    appearance: 'neutral',
    label: 'Disabled',
    hint: "Turned off. Profiles can't use it until it's enabled again.",
  },
}

@Component({
  selector: '[vpnSummary]',
  template: `
    @let vpn = parent.data();
    <section>
      <label appSummary>
        {{ 'Status' | i18n }}
        <span tuiSubtitle>
          @if (vpn) {
            <span
              tuiBadge
              tuiStatus
              [appearance]="statuses[vpn.status].appearance"
            >
              {{ statuses[vpn.status].label | i18n }}
            </span>
          }
        </span>
      </label>
      @if (vpn?.enabled) {
        <label appSummary>
          {{ 'Last handshake' | i18n }}
          <span tuiSubtitle>{{ lastHandshake() || ('Never' | i18n) }}</span>
        </label>
      }
      <label appSummary>
        {{ 'Used by' | i18n }}
        <span tuiSubtitle>{{ vpn?.used_by?.join(', ') || '-' }}</span>
      </label>
    </section>
    @if (vpn) {
      <p
        [class.g-negative]="vpn.status === 'not-responding'"
        [class.g-secondary]="vpn.status !== 'not-responding'"
      >
        {{ statuses[vpn.status].hint | i18n }}
      </p>
    }
    <section>
      <label appSummary>
        {{ 'Connection path' | i18n }}
        <vpn-chain tuiSubtitle [chain]="chain()" />
      </label>
    </section>
  `,
  styles: `
    label:has(vpn-chain) {
      flex: 1;
      min-inline-size: 0;
    }
  `,
  imports: [TuiBadge, TuiStatus, Summary, VPNChain, i18nPipe],
})
export class VPNSummary {
  protected readonly parent = inject(OutboundVPN)
  protected readonly statuses = STATUS
  private readonly relativeTime = new Intl.RelativeTimeFormat(
    inject(i18nService).lang.replace('_', '-'),
  )

  readonly lastHandshake = computed(() => {
    const secs = this.parent.data()?.last_handshake_secs
    if (secs == null) return null
    if (secs < 60) return this.relativeTime.format(-secs, 'second')
    if (secs < 3600)
      return this.relativeTime.format(-Math.floor(secs / 60), 'minute')
    if (secs < 86400)
      return this.relativeTime.format(-Math.floor(secs / 3600), 'hour')
    return this.relativeTime.format(-Math.floor(secs / 86400), 'day')
  })

  readonly chain = computed(() => {
    const label = this.parent.data()?.label
    return label ? buildChain(label, this.parent.service.data() ?? []) : []
  })
}
