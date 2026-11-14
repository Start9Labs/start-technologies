import { inject } from '@angular/core'
import { Routes } from '@angular/router'
import { filter, map } from 'rxjs'
import { AuthGuard } from 'src/app/guards/auth.guard'
import { UnauthGuard } from 'src/app/guards/unauth.guard'
import { StateService, stateNot } from 'src/app/services/state.service'

export const ROUTES: Routes = [
  {
    path: 'unlock',
    canMatch: [
      () =>
        inject(StateService).pipe(
          filter(state => state !== null),
          map(state => state === 'locked'),
        ),
    ],
    loadComponent: () => import('./routes/unlock'),
  },
  {
    path: 'diagnostic',
    canActivate: [stateNot(['locked', 'initializing', 'running'])],
    loadChildren: () => import('./routes/diagnostic/diagnostic.routes'),
  },
  {
    path: 'initializing',
    canActivate: [stateNot(['locked', 'error', 'running'])],
    loadComponent: () => import('./routes/initializing/initializing.page'),
  },
  {
    path: 'login',
    canActivate: [UnauthGuard, stateNot(['locked', 'error', 'initializing'])],
    loadComponent: () => import('./routes/login/login.page'),
  },
  {
    path: '',
    canActivate: [AuthGuard, stateNot(['locked', 'error', 'initializing'])],
    loadChildren: () => import('./routes/portal/portal.routes'),
  },
  {
    path: '**',
    redirectTo: '',
    pathMatch: 'full',
  },
]
