import { Component } from '@angular/core'
import { RouterOutlet, Routes } from '@angular/router'
import { OutboundService } from './service'

@Component({
  template: '<router-outlet />',
  imports: [RouterOutlet],
  providers: [OutboundService],
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
