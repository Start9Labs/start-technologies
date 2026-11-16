import { FactoryProvider } from '@angular/core'
import {
  bufferTime,
  catchError,
  defer,
  delay,
  filter,
  from,
  map,
  Observable,
  repeatWhen,
  scan,
  switchMap,
  timer,
} from 'rxjs'
import type {
  RpcResponseStream,
  RpcStreamOptions,
  T,
} from '@start9labs/start-core'
import { Constructor } from '../types/constructor'
import { convertAnsi } from '../util/convert-ansi'
import { withAbortSignal } from '../util/with-abort'

type FollowLogs = (
  options: RpcStreamOptions,
) => Promise<RpcResponseStream<T.LogFollowResponse, T.LogEntry, null>>

export function provideSetupLogsService<Api>(
  api: Constructor<Api>,
  follow: (api: Api, options: RpcStreamOptions) => ReturnType<FollowLogs>,
): FactoryProvider {
  return {
    provide: SetupLogsService,
    deps: [api],
    useFactory: (instance: Api) =>
      new SetupLogsService(options => follow(instance, options)),
  }
}

/** Disposal prevents late adoption and cancels observation and read-only recovery. */
export class SetupLogsService extends Observable<readonly string[]> {
  private readonly log$ = withAbortSignal(signal =>
    defer(() => this.follow({ signal })).pipe(
      switchMap(({ items }) => from(items)),
      bufferTime(500),
      filter(logs => !!logs.length),
      map(convertAnsi),
      scan((logs: readonly string[], log) => [...logs, log], []),
    ),
  ).pipe(
    repeatWhen(obs => obs.pipe(delay(500))),
    catchError((_, watch$) => timer(500).pipe(switchMap(() => watch$))),
  )

  constructor(private readonly follow: FollowLogs) {
    super(subscriber => this.log$.subscribe(subscriber))
  }
}
