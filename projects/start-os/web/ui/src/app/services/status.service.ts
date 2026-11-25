import { computed, inject, InjectionToken } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { PatchDB } from 'patch-db-client'
import { combineLatest, map, startWith, timer } from 'rxjs'
import { ConnectionService } from './connection.service'
import { NetworkService } from './network.service'
import { DataModel } from './patch-db/data-model'
import { PatchDbCache } from './patch-db/patch-db-cache'
import { StateService } from 'src/app/services/state.service'
import { i18nKey, i18nPipe, i18nService } from '@start9labs/shared'

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

export const LAST_SYNCED = new InjectionToken('', {
  factory: () => {
    const synced = inject(PatchDbCache).synced
    const i18n = inject(i18nService)
    const translate = inject(i18nPipe)
    const now = toSignal(timer(0, 10_000).pipe(map(() => Date.now())), {
      initialValue: Date.now(),
    })

    return computed(() => {
      const time = synced()
      if (!time) return null
      const elapsed = Math.max(0, now() - time) / 1000
      const when =
        elapsed < 60 ? translate.transform('just now') : ago(elapsed, i18n.lang)
      return `${translate.transform('Last synced')} ${when}`
    })
  },
})

const UNITS: readonly [number, Intl.RelativeTimeFormatUnit][] = [
  [60, 'minute'],
  [3600, 'hour'],
  [86_400, 'day'],
  [604_800, 'week'],
  [2_592_000, 'month'],
  [31_536_000, 'year'],
]

function ago(seconds: number, lang: string): string {
  const format = new Intl.RelativeTimeFormat(lang.replace('_', '-'), {
    numeric: 'always',
  })
  const [divisor, unit] =
    [...UNITS].reverse().find(([d]) => seconds >= d) ?? UNITS[0]!
  return format.format(-Math.floor(seconds / divisor), unit)
}

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
