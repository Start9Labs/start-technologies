import { inject } from '@angular/core'
import { Routes } from '@angular/router'
import { AuthService } from 'src/app/services/auth.service'

import { App } from './app'

export const routes: Routes = [
  {
    path: '',
    loadComponent: () => import('./routes/setup-wizard'),
    canMatch: [() => inject(AuthService).setupMode()],
  },
  {
    path: '',
    loadComponent: () => import('./routes/setup'),
    canMatch: [() => !inject(AuthService).initialized()],
  },
  {
    path: '',
    canMatch: [() => inject(AuthService).authenticated()],
    children: [
      {
        path: '',
        component: App,
        children: [
          {
            path: 'wan',
            loadChildren: () => import('./routes/wan'),
          },
          {
            path: 'published-ports',
            loadComponent: () => import('./routes/published-ports'),
          },
          {
            path: 'ethernet',
            loadComponent: () => import('./routes/ethernet'),
          },
          {
            path: 'wifi',
            loadChildren: () => import('./routes/wifi'),
          },
          {
            path: 'outbound',
            loadChildren: () => import('./routes/outbound'),
          },
          {
            path: 'inbound',
            loadChildren: () => import('./routes/inbound'),
          },
          {
            path: 'profiles',
            loadChildren: () => import('./routes/profiles'),
          },
          {
            path: 'lan',
            loadChildren: () => import('./routes/lan'),
          },
          {
            path: 'devices',
            loadChildren: () => import('./routes/devices'),
          },
          {
            path: 'settings',
            loadChildren: () => import('./routes/settings'),
          },
          { path: '**', redirectTo: 'wan' },
        ],
      },
    ],
  },
  {
    path: '',
    loadComponent: () => import('./login'),
    canMatch: [() => !inject(AuthService).authenticated()],
  },
  { path: '**', redirectTo: '' },
]
