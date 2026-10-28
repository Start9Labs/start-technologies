import { Component, inject, Injectable, signal } from '@angular/core'
import { CanActivateFn, IsActiveMatchOptions, Router } from '@angular/router'
import { i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TUI_TRUE_HANDLER } from '@taiga-ui/cdk'
import {
  TuiLoader,
  TuiTitle,
  TuiNotificationService,
  TuiCell,
} from '@taiga-ui/core'
import { PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import {
  BehaviorSubject,
  concat,
  EMPTY,
  exhaustMap,
  from,
  Observable,
  startWith,
  timer,
} from 'rxjs'
import {
  catchError,
  distinctUntilChanged,
  filter,
  map,
  shareReplay,
  skip,
  switchMap,
  takeUntil,
  takeWhile,
  tap,
} from 'rxjs/operators'
import { ServerState } from 'src/app/services/api/api.types'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { NetworkService } from 'src/app/services/network.service'

const OPTIONS: IsActiveMatchOptions = {
  paths: 'subset',
  queryParams: 'exact',
  fragment: 'ignored',
  matrixParams: 'ignored',
}

@Component({
  template: `
    <tui-loader size="m" [inheritColor]="true" />
    <div tuiTitle>
      {{ 'State unknown' | i18n }}
      <span tuiSubtitle>
        {{ 'Trying to reach server' | i18n }}
      </span>
    </div>
  `,
  host: { style: 'padding: 0 0.25rem' },
  imports: [i18nPipe, TuiLoader, TuiTitle],
  hostDirectives: [TuiCell],
})
class DisconnectedToast {}

@Injectable({
  providedIn: 'root',
})
export class StateService extends Observable<ServerState | null> {
  private readonly alerts = inject(TuiNotificationService)
  private readonly i18n = inject(i18nPipe)
  private readonly api = inject(ApiService)
  private readonly router = inject(Router)
  private readonly network$ = inject(NetworkService)
  private readonly trigger$ = new BehaviorSubject(true)

  readonly dnsStatus = signal<T.DnsStatus | null>(null)

  private readonly disconnected$ = this.alerts.open(
    new PolymorpheusComponent(DisconnectedToast),
    { closable: false, appearance: 'negative', icon: '' },
  )

  private readonly reconnected$ = this.alerts.open(
    this.i18n.transform('Connection restored'),
    { label: this.i18n.transform('Server connected'), appearance: 'positive' },
  )

  private readonly stream$ = this.trigger$.pipe(
    switchMap(() => this.network$.pipe(filter(Boolean))),
    switchMap(() =>
      timer(0, 2000).pipe(
        exhaustMap(() =>
          from(this.getState()).pipe(
            tap(({ dnsStatus }) => this.dnsStatus.set(dnsStatus)),
            map(({ state }) => state),
            catchError(() => {
              this.dnsStatus.set(null)
              return EMPTY
            }),
          ),
        ),
        takeWhile(state => state !== 'running', true),
        distinctUntilChanged(),
      ),
    ),
    tap(state => this.handleState(state)),
    startWith(null),
    shareReplay(1),
  )

  constructor() {
    super(subscriber => this.stream$.subscribe(subscriber))

    this.network$
      .pipe(filter(v => !v))
      .subscribe(() => this.retrigger(false, 2000))

    this.trigger$
      .pipe(
        filter(v => !v),
        exhaustMap(() =>
          concat(
            this.disconnected$.pipe(takeUntil(this.stream$.pipe(skip(1)))),
            this.reconnected$,
          ),
        ),
      )
      .subscribe()
  }

  retrigger(gracefully: boolean, delay: number) {
    setTimeout(() => this.trigger$.next(gracefully), delay)
  }

  private async getState() {
    const state = await this.api.rpc.state({}, { timeout: 10000 })
    const dnsStatus =
      state === 'running'
        ? null
        : await this.api.rpc.net.dns
            .status({}, { timeout: 2000 })
            .catch(() => null)
    return { state, dnsStatus }
  }

  private handleState(state: ServerState): void {
    switch (state) {
      case 'initializing':
        this.router.navigate(['initializing'], { replaceUrl: true })
        break
      case 'error':
        this.router.navigate(['diagnostic'], { replaceUrl: true })
        break
      case 'running':
        if (
          this.router.isActive('initializing', OPTIONS) ||
          this.router.isActive('diagnostic', OPTIONS)
        ) {
          this.router.navigate([''], { replaceUrl: true })
        }

        break
    }
  }
}

export function stateNot(state: ServerState[]): CanActivateFn {
  return () =>
    inject(StateService).pipe(
      filter(current => !current || !state.includes(current)),
      map(TUI_TRUE_HANDLER),
    )
}
