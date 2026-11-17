import { inject, InjectionToken } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { PatchDB } from 'patch-db-client'
import { combineLatest, map, startWith } from 'rxjs'
import { ConnectionService } from './connection.service'
import { NetworkService } from './network.service'
import { DataModel } from './patch-db/data-model'
import { StateService } from 'src/app/services/state.service'
import { i18nKey } from '@start9labs/shared'

export const STATUS = new InjectionToken('', {
  factory: () =>
    toSignal(
      combineLatest({
        network: inject(NetworkService),
        live: inject(ConnectionService),
        reachable: inject(StateService).reachable$,
        status: inject<PatchDB<DataModel>>(PatchDB)
          .watch$('serverInfo', 'statusInfo')
          .pipe(startWith({ restarting: false, shuttingDown: false })),
      }).pipe(
        map(({ network, live, reachable, status }) => {
          if (!network) return OFFLINE
          if (status.shuttingDown) return SHUTTING_DOWN
          if (status.restarting) return RESTARTING
          if (!live) return reachable === false ? UNREACHABLE : CONNECTING

          return CONNECTED
        }),
      ),
      { initialValue: CONNECTING },
    ),
})

const OFFLINE: ServerStatus = {
  message: 'No Internet',
  color: 'var(--tui-status-negative)',
  icon: '@tui.cloud-off',
  status: 'error',
}
const CONNECTING: ServerStatus = {
  message: 'Connecting',
  color: 'var(--tui-status-warning)',
  icon: '@tui.cloud-off',
  status: 'warning',
}
const UNREACHABLE: ServerStatus = {
  message: "Can't reach server",
  color: 'var(--tui-status-negative)',
  icon: '@tui.cloud-off',
  status: 'error',
}
const SHUTTING_DOWN: ServerStatus = {
  message: 'Shutting down',
  color: 'var(--tui-status-neutral)',
  icon: '@tui.power',
  status: 'neutral',
}
const RESTARTING: ServerStatus = {
  message: 'Restarting',
  color: 'var(--tui-status-neutral)',
  icon: '@tui.power',
  status: 'neutral',
}
const CONNECTED: ServerStatus = {
  message: 'Connected',
  color: 'var(--tui-status-positive)',
  icon: '@tui.cloud',
  status: 'success',
}

type ServerStatus = {
  message: i18nKey
  color: string
  icon: string
  status: 'error' | 'warning' | 'neutral' | 'success'
}
