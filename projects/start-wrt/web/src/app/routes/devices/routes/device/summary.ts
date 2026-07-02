import { Component, computed, inject } from '@angular/core'
import { TuiFormatNumberPipe, TuiIcon, TuiLink } from '@taiga-ui/core'
import { Summary } from 'src/app/components/summary'

import DeviceDetail from '.'
import { DataUsageChart } from './data-usage-chart'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  selector: '[deviceSummary]',
  template: `
    <section>
      <div appSummary>
        {{ 'MAC Address' | i18n }}
        <span tuiSubtitle>{{ parent.data()?.mac || '-' }}</span>
      </div>
      <div appSummary>
        {{ 'Status' | i18n }}
        <span tuiSubtitle>
          @switch (parent.data()?.status) {
            @case ('online') {
              <tui-icon icon="@tui.circle-check" class="g-positive" />
              {{ 'Online' | i18n }}
            }
            @case ('offline') {
              <tui-icon icon="@tui.circle-minus" />
              {{ 'Offline' | i18n }}
            }
          }
        </span>
      </div>
      @if (parent.data()?.connection) {
        <div appSummary>
          {{ 'Connection' | i18n }}
          <span tuiSubtitle>
            <tui-icon [icon]="connectionIcon()" />
            {{ parent.data()?.connection }}
          </span>
        </div>
      }
      @if (parent.data()?.securityProfile) {
        <div appSummary>
          {{ 'Security Profile' | i18n }}
          <span tuiSubtitle>
            <a tuiLink>
              <strong>{{ parent.data()?.securityProfile }}</strong>
            </a>
          </span>
        </div>
      }
      @if (parent.data()?.ipv4) {
        <div appSummary>
          {{ 'IPv4 Address' | i18n }}
          <span tuiSubtitle>
            @if (parent.data()?.ipv4Static) {
              <tui-icon icon="@tui.lock" />
            }
            {{ parent.data()?.ipv4 }}
          </span>
        </div>
      }
      @if (parent.data()?.ipv6) {
        <div appSummary>
          {{ 'IPv6 Address' | i18n }}
          <span tuiSubtitle>
            @if (parent.data()?.ipv6Static) {
              <tui-icon icon="@tui.lock" />
            }
            {{ parent.data()?.ipv6 }}
          </span>
        </div>
      }
      <div appSummary>
        {{ 'Speed' | i18n }}
        <span tuiSubtitle>
          <tui-icon icon="@tui.arrow-up" />
          {{ parent.data()?.speed?.up ?? 0 | tuiFormatNumber }}
          <small>MB/s</small>
        </span>
        <span tuiSubtitle>
          <tui-icon icon="@tui.arrow-down" />
          {{ parent.data()?.speed?.down ?? 0 | tuiFormatNumber }}
          <small>MB/s</small>
        </span>
      </div>
    </section>
    <app-data-usage-chart
      [mac]="parent.data()?.mac ?? ''"
      [service]="parent.service"
      [loading]="!parent.data()"
    />
  `,
  styles: `
    tui-icon {
      font-size: 1rem;
    }

    [tuiSubtitle] {
      gap: 0.25rem;
    }
  `,
  imports: [
    TuiIcon,
    TuiLink,
    TuiFormatNumberPipe,
    Summary,
    DataUsageChart,
    i18nPipe,
  ],
})
export class DeviceSummary {
  protected readonly parent = inject(DeviceDetail)

  readonly connectionIcon = computed(() => {
    const connection = this.parent.data()?.connection?.toLowerCase() ?? ''
    if (connection.includes('ethernet') || connection.includes('eth')) {
      return '@tui.cable'
    }
    if (connection.includes('wi-fi') || connection.includes('wifi')) {
      return '@tui.wifi'
    }
    if (connection.startsWith('vpn')) {
      return '@tui.shield'
    }
    return '@tui.monitor'
  })
}
