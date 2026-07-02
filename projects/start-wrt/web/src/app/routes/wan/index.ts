import { Component } from '@angular/core'
import {
  RouterLink,
  RouterLinkActive,
  RouterOutlet,
  Routes,
} from '@angular/router'
import { TuiTitle } from '@taiga-ui/core'
import { TuiTabs } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { ToCamelPipe } from 'src/app/pipes/to-camel.pipe'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2>{{ 'Internet (WAN)' | i18n }}</h2>
      </hgroup>
    </header>
    <tui-tabs>
      @for (tab of tabs; track $index) {
        <a tuiTab routerLinkActive [routerLink]="tab | toCamel">
          {{ tab | i18n }}
        </a>
      }
    </tui-tabs>
    <router-outlet />
  `,
  imports: [
    RouterOutlet,
    RouterLink,
    RouterLinkActive,
    TuiHeader,
    TuiTitle,
    TuiTabs,
    ToCamelPipe,
    i18nPipe,
  ],
})
class Wan {
  protected readonly tabs = [
    'IPv4',
    'IPv6',
    'DNS',
    'MAC Address',
    'Dynamic DNS',
  ]
}

export default [
  {
    path: '',
    component: Wan,
    children: [
      {
        path: 'ipv4',
        loadComponent: () => import('./routes/ipv4'),
      },
      {
        path: 'ipv6',
        loadComponent: () => import('./routes/ipv6'),
      },
      {
        path: 'dns',
        loadComponent: () => import('./routes/dns'),
      },
      {
        path: 'mac-address',
        loadComponent: () => import('./routes/mac'),
      },
      {
        path: 'dynamic-dns',
        loadComponent: () => import('./routes/ddns'),
      },
      {
        path: '**',
        redirectTo: 'ipv4',
      },
    ],
  },
] satisfies Routes
