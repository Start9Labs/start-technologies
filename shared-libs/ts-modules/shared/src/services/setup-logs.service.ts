import { FactoryProvider } from '@angular/core'
import {
  bufferTime,
  catchError,
  defer,
  delay,
  filter,
  map,
  Observable,
  repeatWhen,
  scan,
  switchMap,
  timer,
} from 'rxjs'
import { T } from '@start9labs/start-core'
import { Constructor } from '../types/constructor'
import { convertAnsi } from '../util/convert-ansi'

interface LogsApi {
  openWebsocket$: (guid: string) => Observable<T.LogEntry>
}

/** `follow` opens the server's log stream for the app's current state. */
export function provideSetupLogsService<Api extends LogsApi>(
  api: Constructor<Api>,
  follow: (api: Api) => Promise<T.LogFollowResponse>,
): FactoryProvider {
  return {
    provide: SetupLogsService,
    deps: [api],
    useFactory: (instance: Api) =>
      new SetupLogsService(
        () => follow(instance),
        guid => instance.openWebsocket$(guid),
      ),
  }
}

export class SetupLogsService extends Observable<readonly string[]> {
  private readonly log$ = defer(() => this.follow()).pipe(
    switchMap(({ guid }) => this.websocket(guid)),
    bufferTime(500),
    filter(logs => !!logs.length),
    map(convertAnsi),
    scan((logs: readonly string[], log) => [...logs, log], []),
    repeatWhen(obs => obs.pipe(delay(500))),
    catchError((_, watch$) => timer(500).pipe(switchMap(() => watch$))),
  )

  constructor(
    private readonly follow: () => Promise<T.LogFollowResponse>,
    private readonly websocket: (guid: string) => Observable<T.LogEntry>,
  ) {
    super(subscriber => this.log$.subscribe(subscriber))
  }
}
