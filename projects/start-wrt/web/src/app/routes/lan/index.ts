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
        <h2>{{ 'Network (LAN)' | i18n }}</h2>
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
class Lan {
  protected readonly tabs = ['IPv4', 'IPv6']
}

export default [
  {
    path: '',
    component: Lan,
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
        path: '**',
        redirectTo: 'ipv4',
      },
    ],
  },
] satisfies Routes
