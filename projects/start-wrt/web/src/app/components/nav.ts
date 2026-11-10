import { KeyValuePipe } from '@angular/common'
import { Component, inject } from '@angular/core'
import { RouterLink } from '@angular/router'
import { TuiIcon } from '@taiga-ui/core'
import { TuiNavigation } from '@taiga-ui/layout'
import { SystemService } from 'src/app/services/system.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

const MENU = {
  Internet: [
    {
      name: 'WAN Settings',
      icon: '@tui.globe',
      link: 'wan',
    },
    {
      name: 'Published Ports',
      icon: '@tui.door-open',
      link: 'published-ports',
    },
    {
      name: 'Outbound VPNs',
      icon: '@tui.hat-glasses',
      link: 'outbound',
    },
  ],
  Network: [
    {
      name: 'LAN Settings',
      icon: '@tui.network',
      link: 'lan',
    },
    {
      name: 'Devices',
      icon: '@tui.monitor-smartphone',
      link: 'devices',
    },
  ],
  'Security Profiles': [
    {
      name: 'Profiles',
      icon: '@tui.user-lock',
      link: 'profiles',
    },
  ],
  'Points of entry': [
    {
      name: 'Ethernet',
      icon: '@tui.ethernet-port',
      link: 'ethernet',
    },
    {
      name: 'Wi-Fi',
      icon: '@tui.wifi',
      link: 'wifi',
    },
    {
      name: 'Inbound VPNs',
      icon: '@tui.hard-drive-download',
      link: 'inbound',
    },
  ],
  System: [
    {
      name: 'Settings',
      icon: '@tui.settings',
      link: 'settings',
    },
  ],
} as const

@Component({
  selector: '[appNav]',
  template: `
    @for (item of routes | keyvalue: asIs; track $index) {
      <span>{{ item.key | i18n }}</span>
      @for (route of item.value; track $index) {
        <a tuiAsideItem [iconStart]="route.icon" [routerLink]="route.link">
          {{ route.name | i18n }}
          @if (route.link === 'settings' && system.updateAvailable()) {
            <tui-icon class="g-positive" icon="@tui.rocket" />
          }
        </a>
      }
      @if (!$last) {
        <hr />
      }
    }
  `,
  styles: `
    @use '@taiga-ui/styles/utils' as taiga;

    :host-context(aside._expanded) span {
      font: var(--tui-typography-body-s);
      padding-block: 0.5em;
    }

    span {
      @include taiga.transition(all);

      display: block;
      padding: 0 0.5rem;
      color: var(--tui-text-secondary);
      text-transform: uppercase;
      font-size: 0;
    }

    tui-icon {
      margin-inline-start: auto !important;
    }
  `,
  imports: [TuiIcon, RouterLink, KeyValuePipe, TuiNavigation, i18nPipe],
})
export class Nav {
  protected readonly system = inject(SystemService)
  protected readonly routes = MENU

  protected asIs(): number {
    return 0
  }
}
