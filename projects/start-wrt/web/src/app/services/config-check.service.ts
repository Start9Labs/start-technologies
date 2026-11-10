import { computed, inject, Injectable, signal } from '@angular/core'
import { takeUntilDestroyed, toSignal } from '@angular/core/rxjs-interop'
import { NavigationEnd, Router } from '@angular/router'
import {
  catchError,
  EMPTY,
  filter,
  from,
  map,
  merge,
  Subject,
  switchMap,
  timer,
} from 'rxjs'
import { ApiService, ConfigFinding, ConfigView } from './api/api.service'
import { AuthService } from './auth.service'

const VIEWS = [
  'wan',
  'published-ports',
  'outbound',
  'lan',
  'devices',
  'profiles',
  'ethernet',
  'wifi',
  'inbound',
] as const satisfies readonly ConfigView[]

/** RPC errors raised by configuration StartWRT cannot read or model. */
const UNREPRESENTABLE = new Set([
  1003, 1004, 1005, 1006, 1009, 1010, 1011, 1012, 1022,
])

@Injectable({ providedIn: 'root' })
export class ConfigCheckService {
  private readonly api = inject(ApiService)
  private readonly auth = inject(AuthService)
  private readonly router = inject(Router)
  private readonly check$ = new Subject<void>()
  private readonly navigation$ = this.router.events.pipe(
    filter(e => e instanceof NavigationEnd),
  )
  private readonly url = toSignal(
    this.navigation$.pipe(map(() => this.router.url)),
    { initialValue: this.router.url },
  )
  private readonly findings = toSignal(
    merge(this.check$, this.navigation$, timer(0, 30_000)).pipe(
      filter(() => this.auth.authenticated()),
      switchMap(() =>
        from(this.api.configCheck()).pipe(
          map(({ findings }) => findings),
          catchError(() => EMPTY),
        ),
      ),
    ),
    { initialValue: [] as ConfigFinding[] },
  )
  private readonly failure = signal<{
    view: ConfigView
    message: string
  } | null>(null)
  /** A page that still fails reports again once it loads. */
  private readonly _ = this.navigation$
    .pipe(takeUntilDestroyed())
    .subscribe(() => this.failure.set(null))

  readonly view = computed(() => {
    const [path = ''] = this.url().split(/[?#]/)
    return VIEWS.find(view => view === path.split('/')[1]) ?? null
  })
  private readonly current = computed(() => {
    const view = this.view()
    return view ? this.findings().filter(f => f.views.includes(view)) : []
  })
  /** Findings shown in place of the page's settings. */
  readonly blocking = computed(() => this.current().filter(f => !f.notice))
  /** Findings shown above the page's settings. */
  readonly notices = computed(() => this.current().filter(f => f.notice))
  readonly loadError = computed(() => {
    const failure = this.failure()
    return failure && failure.view === this.view() ? failure.message : null
  })
  readonly blocked = computed(
    () => this.blocking().length > 0 || this.loadError() !== null,
  )

  /** Takes over the page when its load failed on configuration. */
  report(error: unknown): boolean {
    const { code, message } = (error ?? {}) as {
      code?: number
      message?: string
    }
    const view = this.view()
    if (!view || code === undefined || !UNREPRESENTABLE.has(code)) {
      return false
    }
    this.failure.set({ view, message: message || '' })
    this.check$.next()
    return true
  }

  recheck(): void {
    this.failure.set(null)
    this.check$.next()
  }
}
