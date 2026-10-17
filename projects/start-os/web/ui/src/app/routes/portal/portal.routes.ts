import { Routes } from '@angular/router'

import { SYSTEM_UTILITIES } from 'src/app/utils/system-utilities'
import { titleResolver } from 'src/app/utils/title-resolver'
import { toRouterLink } from 'src/app/utils/to-router-link'
import { PortalComponent } from './portal.component'

const ROUTES: Routes = [
  {
    path: '',
    component: PortalComponent,
    children: [
      {
        redirectTo: 'services',
        pathMatch: 'full',
        path: '',
      },
      {
        path: 'services',
        data: { title: 'Services' },
        title: titleResolver,
        loadChildren: () => import('./routes/services/services.routes'),
      },
      {
        path: 'backups',
        redirectTo: '/system/backups',
        pathMatch: 'full',
      },
      {
        path: 'backups/:section',
        redirectTo: '/system/backups/:section',
        pathMatch: 'full',
      },
      {
        title: titleResolver,
        path: 'marketplace',
        loadChildren: () => import('./routes/marketplace/marketplace.routes'),
        data: toNavigationItem('marketplace'),
      },
      {
        title: titleResolver,
        path: 'system',
        loadChildren: () => import('./routes/system/system.routes'),
        data: toNavigationItem('system'),
      },
      {
        title: titleResolver,
        path: 'notifications',
        loadComponent: () =>
          import('./routes/notifications/notifications.component'),
        data: toNavigationItem('notifications'),
      },
      {
        title: titleResolver,
        path: 'sideload',
        loadComponent: () => import('./routes/sideload/sideload.component'),
        data: toNavigationItem('sideload'),
      },
      {
        title: titleResolver,
        path: 'updates',
        loadComponent: () => import('./routes/updates/updates.component'),
        data: toNavigationItem('updates'),
      },
      {
        title: titleResolver,
        path: 'metrics',
        loadComponent: () => import('./routes/metrics/metrics.component'),
        data: toNavigationItem('metrics'),
      },
    ],
  },
]

export default ROUTES

function toNavigationItem(id: string) {
  const { icon, title } = SYSTEM_UTILITIES[id] || {}

  return {
    icon,
    title,
    routerLink: toRouterLink(id),
  }
}
