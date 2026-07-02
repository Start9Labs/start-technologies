import { Component, computed, inject } from '@angular/core'
import { TuiIcon } from '@taiga-ui/core'
import { TuiAvatar, TuiAvatarLabeled, TuiBadge, TuiStatus } from '@taiga-ui/kit'
import { Summary } from 'src/app/components/summary'
import { buildConnectionPath } from 'src/app/routes/outbound/utils'
import OutboundVPN from 'src/app/routes/outbound/routes/vpn/index'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  selector: '[vpnSummary]',
  template: `
    <section>
      <label appSummary>
        {{ 'Status' | i18n }}
        <span tuiSubtitle>
          @if (parent.data()?.enabled) {
            <span tuiBadge tuiStatus appearance="positive">
              {{ 'Connected' | i18n }}
            </span>
          } @else {
            <span tuiBadge tuiStatus appearance="neutral">
              {{ 'Disabled' | i18n }}
            </span>
          }
        </span>
      </label>
      <label appSummary>
        {{ 'Used by' | i18n }}
        <span tuiSubtitle>{{ parent.data()?.used_by?.join(', ') || '-' }}</span>
      </label>
    </section>
    <section>
      <label appSummary>
        {{ 'Connection path' | i18n }}
        <span tuiSubtitle>
          <tui-avatar-labeled [label]="'Client' | i18n">
            <span
              tuiAvatar="@tui.monitor-smartphone"
              size="m"
              appearance="action"
            ></span>
          </tui-avatar-labeled>
          <tui-icon icon="@tui.arrow-right" />
          @for (step of connectionPath(); track $index; let last = $last) {
            @if (step === 'Internet') {
              <tui-avatar-labeled [label]="'Internet' | i18n">
                <span
                  tuiAvatar="@tui.globe"
                  size="m"
                  appearance="action"
                  class="g-positive"
                ></span>
              </tui-avatar-labeled>
            } @else {
              <tui-avatar-labeled [label]="step">
                <span
                  appearance="action"
                  size="m"
                  tuiAvatar="@tui.globe-lock"
                ></span>
              </tui-avatar-labeled>
              <tui-icon icon="@tui.arrow-right" />
            }
          }
        </span>
      </label>
    </section>
  `,
  styles: `
    tui-avatar {
      margin: 0;
    }

    [tuiSubtitle]:has(tui-avatar-labeled) {
      color: var(--tui-text-primary);
    }
  `,
  imports: [
    TuiBadge,
    TuiStatus,
    Summary,
    TuiAvatarLabeled,
    TuiAvatar,
    TuiIcon,
    i18nPipe,
  ],
})
export class VPNSummary {
  protected readonly parent = inject(OutboundVPN)

  readonly connectionPath = computed(() => {
    const data = this.parent.data()
    const allVpns = this.parent.service.data() ?? []
    if (!data || allVpns.length === 0) {
      return ['-', 'Internet']
    }
    return buildConnectionPath(data.label, allVpns)
  })
}
