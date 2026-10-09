import { WA_IS_MOBILE } from '@ng-web-apis/platform'
import { inject } from '@angular/core'
import {
  ActivatedRouteSnapshot,
  Router,
  RouterStateSnapshot,
  Routes,
} from '@angular/router'

import { titleResolver } from 'src/app/utils/title-resolver'
import { SystemComponent } from './system.component'

export default [
  {
    path: '',
    component: SystemComponent,
    canActivate: [
      ({ firstChild }: ActivatedRouteSnapshot, state: RouterStateSnapshot) =>
        !!firstChild ||
        inject(WA_IS_MOBILE) ||
        inject(Router).parseUrl(`${state.url}/general`),
    ],
    children: [
      {
        path: 'general',
        title: titleResolver,
        loadComponent: () => import('./routes/general/general.component'),
      },
      {
        path: 'email',
        title: titleResolver,
        loadComponent: () => import('./routes/smtp/smtp.component'),
      },
      {
        path: 'backups',
        title: titleResolver,
        loadChildren: () => import('../backups/backups.routes'),
        data: { title: 'Backups' },
      },
      {
        path: 'backup',
        redirectTo: 'backups/manual',
        pathMatch: 'full',
      },
      {
        path: 'restore',
        redirectTo: 'backups/restore',
        pathMatch: 'full',
      },
      {
        path: 'interfaces',
        title: titleResolver,
        loadComponent: () => import('./routes/startos-ui/startos-ui.component'),
      },
      {
        path: 'wifi',
        title: titleResolver,
        loadComponent: () => import('./routes/wifi/wifi.component'),
      },
      {
        path: 'sessions',
        title: titleResolver,
        loadComponent: () => import('./routes/sessions/sessions.component'),
      },
      {
        path: 'ssh',
        title: titleResolver,
        loadComponent: () => import('./routes/ssh/ssh.component'),
      },
      {
        path: 'password',
        title: titleResolver,
        loadComponent: () => import('./routes/password/password.component'),
      },
      {
        path: 'gateways',
        loadComponent: () => import('./routes/gateways/gateways.component'),
      },
      // {
      //   path: 'authorities',
      //   title: titleResolver,
      //   loadComponent: () =>
      //     import('./routes/authorities/authorities.component'),
      // },
      {
        path: 'dns',
        title: titleResolver,
        loadComponent: () => import('./routes/dns/dns.component'),
      },
      {
        path: 'os-logs',
        title: titleResolver,
        data: { title: 'OS Logs' },
        loadComponent: () => import('../logs/routes/os.component'),
      },
      {
        path: 'kernel-logs',
        title: titleResolver,
        data: { title: 'Kernel Logs' },
        loadComponent: () => import('../logs/routes/kernel.component'),
      },
    ],
  },
] satisfies Routes
