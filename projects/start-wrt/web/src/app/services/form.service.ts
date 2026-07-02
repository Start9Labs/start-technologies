import { inject, Type } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { TuiNotificationService } from '@taiga-ui/core'
import {
  catchError,
  EMPTY,
  firstValueFrom,
  from,
  merge,
  share,
  Subject,
  switchMap,
  timer,
} from 'rxjs'
import { ActionService } from 'src/app/services/action.service'
import {
  ConnectionService,
  isNetworkError,
} from 'src/app/services/connection.service'

export type FormRawValue<T> = T extends { getRawValue(): infer R } ? R : never

export abstract class FormService<T> {
  private readonly load$ = new Subject<void>()
  private readonly alerts = inject(TuiNotificationService)
  protected readonly connection = inject(ConnectionService)
  private readonly value$ = merge(this.load$, timer(0, 5000)).pipe(
    switchMap(() =>
      from(this.load()).pipe(
        catchError(e => {
          console.error(e)
          // Network drops funnel into the single global "Reconnecting"
          // indicator instead of a per-poll "Unknown Error" toast; auth
          // failures are handled by the RPC layer (logout).
          if (isNetworkError(e)) {
            this.connection.reportUnreachable()
            return EMPTY
          }
          return e?.code === 34
            ? EMPTY
            : this.alerts.open<never>(e?.message || e, {
                appearance: 'negative',
              })
        }),
      ),
    ),
    share(),
  )

  protected readonly actions = inject(ActionService)
  readonly data = toSignal(this.value$)

  abstract load(): Promise<T>
  abstract store(data: T): Promise<void>

  refresh(): void {
    this.load$.next()
  }

  protected async refreshAndWait(): Promise<void> {
    this.refresh()
    await firstValueFrom(this.value$)
  }

  async save(data: T): Promise<boolean> {
    return this.actions.run(async () => {
      await this.store(data)
      await this.refreshAndWait()
    })
  }
}

export function provideFormService<T = any>(useExisting: Type<FormService<T>>) {
  return [useExisting, { provide: FormService, useExisting }]
}

export function injectFormService<T>(): FormService<T> {
  return inject(FormService)
}
