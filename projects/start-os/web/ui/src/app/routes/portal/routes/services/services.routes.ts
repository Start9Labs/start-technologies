import { Routes } from '@angular/router'
import { titleResolver } from 'src/app/utils/title-resolver'

import { ServiceOutletComponent } from './routes/outlet.component'
import { ServiceRoute } from './routes/service.component'

export const ROUTES: Routes = [
  {
    path: ':pkgId',
    title: titleResolver,
    component: ServiceOutletComponent,
    children: [
      {
        path: '',
        component: ServiceRoute,
      },
      {
        path: 'interfaces',
        children: [
          {
            path: '',
            loadComponent: () => import('./routes/interfaces.component'),
          },
          {
            path: ':interfaceId',
            loadComponent: () => import('./routes/interface.component'),
          },
        ],
      },
      {
        path: 'outbound-traffic',
        loadComponent: () => import('./routes/outbound-traffic'),
      },
      {
        path: 'actions',
        loadComponent: () => import('./routes/actions.component'),
      },
      {
        path: 'logs',
        loadComponent: () => import('./routes/logs.component'),
      },
      {
        path: 'instructions',
        loadComponent: () => import('./routes/instructions.component'),
      },
      {
        path: 'about',
        loadComponent: () => import('./routes/about.component'),
      },
    ],
  },
  {
    path: '',
    pathMatch: 'full',
    loadComponent: () => import('./dashboard/dashboard.component'),
  },
]

export default ROUTES
