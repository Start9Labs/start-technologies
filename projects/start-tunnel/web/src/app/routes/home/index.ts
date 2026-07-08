import { Routes } from '@angular/router'
import { Outlet } from 'src/app/routes/home/components/outlet'

export default [
  {
    path: '',
    component: Outlet,
    children: [
      {
        path: 'subnets',
        loadComponent: () => import('./routes/subnets'),
        title: 'Subnets',
      },
      {
        path: 'devices',
        loadComponent: () => import('./routes/devices'),
        title: 'Devices',
      },
      {
        path: 'published-ports',
        loadComponent: () => import('./routes/published-ports'),
        title: 'Published Ports',
      },
      {
        path: 'dns',
        loadComponent: () => import('./routes/dns'),
        title: 'DNS',
      },
      {
        path: 'settings',
        loadComponent: () => import('./routes/settings'),
        title: 'Settings',
      },
      { path: '**', redirectTo: 'subnets' },
    ],
  },
] satisfies Routes
