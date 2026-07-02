import { Component } from '@angular/core'
import { RouterOutlet, Routes } from '@angular/router'

@Component({
  template: '<router-outlet />',
  imports: [RouterOutlet],
})
class Outbound {}

export default [
  {
    path: '',
    component: Outbound,
    children: [
      {
        path: '',
        loadComponent: () => import('./routes/table'),
      },
      {
        path: 'vpn',
        loadComponent: () => import('./routes/vpn'),
      },
    ],
  },
] satisfies Routes
