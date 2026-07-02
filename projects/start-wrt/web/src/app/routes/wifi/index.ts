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
import { provideFormService } from 'src/app/services/form.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { WifiService } from './service'

@Component({
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2>{{ 'Wi-Fi' | i18n }}</h2>
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
class Wifi {
  protected readonly tabs = ['Passwords', 'Blackout Schedule', 'Settings']
}

export default [
  {
    path: '',
    component: Wifi,
    providers: [provideFormService(WifiService)],
    children: [
      {
        path: 'passwords',
        loadComponent: () => import('./routes/passwords'),
      },
      {
        path: 'blackout-schedule',
        loadComponent: () => import('./routes/blackout-schedule'),
      },
      {
        path: 'settings',
        loadComponent: () => import('./routes/settings'),
      },
      {
        path: '**',
        redirectTo: 'passwords',
      },
    ],
  },
] satisfies Routes
