import { CommonModule } from '@angular/common'
import { Component, inject, viewChild } from '@angular/core'
import { RouterLink } from '@angular/router'
import { i18nPipe, TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton } from '@taiga-ui/core'
import { from, map, merge, Observable, Subject } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { TitleDirective } from 'src/app/services/title.service'
import { SessionsTableComponent } from './table.component'

@Component({
  template: `
    <ng-container *title>
      <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
        {{ 'Back' | i18n }}
      </a>
      {{ 'Active Sessions' | i18n }}
    </ng-container>

    <section class="g-card">
      <header>{{ 'Current session' | i18n }}</header>
      <div [single]="true" [sessions]="current$ | async"></div>
    </section>

    @let others = other$ | async;
    <section class="g-card">
      <header>
        {{ 'Other sessions' | i18n }}
        @if (sessions()?.selected()?.length) {
          <button
            tuiButton
            class="terminate"
            size="xs"
            appearance="primary-destructive"
            (click)="terminate(others || [])"
          >
            {{ 'Terminate' | i18n }}
          </button>
        }
      </header>
      <div
        #table
        [sessions]="others"
        (terminateSelected)="terminate(others || [])"
      ></div>
    </section>
  `,
  styles: `
    :host {
      max-width: 80rem;
    }

    :host-context(tui-root:not(._mobile)) .terminate {
      display: none;
    }
  `,
  imports: [
    CommonModule,
    TuiButton,
    SessionsTableComponent,
    RouterLink,
    TitleDirective,
    i18nPipe,
  ],
})
export default class SystemSessionsComponent {
  private readonly tasks = inject(TaskService)
  private readonly api = inject(ApiService)
  private readonly sessions$ = from(this.api.rpc.auth.session.list({}))
  private readonly local$ = new Subject<readonly SessionWithId[]>()

  protected sessions = viewChild<SessionsTableComponent<SessionWithId>>('table')

  readonly current$ = this.sessions$.pipe(
    map(s => {
      const current = s.current ? s.sessions[s.current] : undefined

      return current ? [current] : []
    }),
  )
  readonly other$: Observable<readonly SessionWithId[]> = merge(
    this.local$,
    this.sessions$.pipe(
      map(s =>
        Object.entries(s.sessions)
          .filter(([id, _]) => id !== s.current)
          .map(([id, session]) => ({
            id,
            ...session,
          }))
          .sort(
            (a, b) =>
              new Date(b.lastActive).valueOf() -
              new Date(a.lastActive).valueOf(),
          ),
      ),
    ),
  )

  async terminate(all: readonly SessionWithId[]) {
    const ids =
      this.sessions()
        ?.selected()
        .map(s => s.id) || []

    this.tasks.run(async () => {
      await this.api.rpc.auth.session.kill({ ids })
      this.local$.next(all.filter(s => !ids.includes(s.id)))
      this.sessions()?.selected.set([])
    }, 'Terminating sessions')
  }
}

interface SessionWithId extends T.Session {
  id: string
}
