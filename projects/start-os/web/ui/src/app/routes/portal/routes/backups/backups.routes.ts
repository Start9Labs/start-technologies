import {
  ActivatedRouteSnapshot,
  CanDeactivateFn,
  Routes,
} from '@angular/router'

import { titleResolver } from 'src/app/utils/title-resolver'
import type BackupsComponent from './backups.component'

const confirmBackupExit: CanDeactivateFn<BackupsComponent> = component =>
  component.canDeactivate()

export default [
  ...['', 'automatic', 'manual', 'restore', 'locations', 'history'].map(
    panel => ({
      path: panel,
      data: { panel: panel || null },
      title: titleResolver,
      loadComponent: () => import('./backups.component'),
      canDeactivate: [confirmBackupExit],
      runGuardsAndResolvers: (
        from: ActivatedRouteSnapshot,
        to: ActivatedRouteSnapshot,
      ) => from.queryParamMap.get('panel') !== to.queryParamMap.get('panel'),
    }),
  ),
  {
    path: '**',
    redirectTo: '',
  },
] satisfies Routes
