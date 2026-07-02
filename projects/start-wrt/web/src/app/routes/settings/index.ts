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
        <h2>{{ 'System Settings' | i18n }}</h2>
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
  styles: `
    :host {
      height: stretch;
    }
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
class Settings {
  protected readonly tabs = [
    'General',
    'Password',
    'SSH Keys',
    'Activity',
    'Logs',
    'Backup',
    'Advanced',
  ]
}

export default [
  {
    path: '',
    component: Settings,
    children: [
      {
        path: 'general',
        loadComponent: () => import('./routes/general'),
      },
      {
        path: 'password',
        loadComponent: () => import('./routes/password'),
      },
      {
        path: 'ssh-keys',
        loadComponent: () => import('./routes/ssh-keys'),
      },
      {
        path: 'activity',
        loadComponent: () => import('./routes/activity'),
      },
      {
        path: 'logs',
        loadComponent: () => import('./routes/logs'),
      },
      {
        path: 'backup',
        loadComponent: () => import('./routes/backup'),
      },
      {
        path: 'advanced',
        loadComponent: () => import('./routes/advanced'),
      },
      {
        path: '**',
        redirectTo: 'general',
      },
    ],
  },
] satisfies Routes
