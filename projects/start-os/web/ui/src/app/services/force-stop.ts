import type { T } from '@start9labs/start-core'
import {
  combineLatest,
  defaultIfEmpty,
  distinctUntilChanged,
  firstValueFrom,
  map,
  Observable,
} from 'rxjs'

export function forceStopAt(status: T.StatusInfo, now: number): string | null {
  return status.desired.main === 'stopped' &&
    status.started &&
    status.forceStopAt &&
    new Date(status.forceStopAt).valueOf() <= now
    ? status.forceStopAt
    : null
}

export function forceStopAt$(
  status$: Observable<T.StatusInfo>,
  now$: Observable<{ now: number }>,
): Observable<string | null> {
  return combineLatest([status$, now$]).pipe(
    map(([status, { now }]) => forceStopAt(status, now)),
    distinctUntilChanged(),
  )
}

export async function confirmForceStop(
  params: T.ForceStopParams,
  confirm: () => Observable<boolean>,
  stop: (params: T.ForceStopParams) => Promise<unknown>,
): Promise<void> {
  const request = { ...params }
  if (await firstValueFrom(confirm().pipe(defaultIfEmpty(false)))) {
    await stop(request)
  }
}
