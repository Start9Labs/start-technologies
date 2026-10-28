import {
  Component,
  ElementRef,
  inject,
  OnInit,
  ViewChild,
  signal,
} from '@angular/core'
import { RouterLink } from '@angular/router'
import {
  WA_INTERSECTION_ROOT,
  WaIntersectionObserver,
} from '@ng-web-apis/intersection-observer'
import { WaMutationObserver } from '@ng-web-apis/mutation-observer'
import { convertAnsi, ErrorService } from '@start9labs/shared'
import { tuiProvide } from '@taiga-ui/cdk'
import { TuiButton, TuiLoader, TuiScrollbar } from '@taiga-ui/core'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { ApiService } from 'src/app/services/api/embassy-api.service'

@Component({
  template: `
    <a
      routerLink="../"
      tuiButton
      iconStart="@tui.chevron-left"
      appearance="icon"
      [style.align-self]="'flex-start'"
    >
      Back
    </a>
    <tui-scrollbar childList subtree (waMutationObserver)="restoreScroll()">
      <section
        class="top"
        waIntersectionObserver
        (waIntersectionObservee)="onTop(!!$event[0]?.isIntersecting)"
      >
        @if (loading()) {
          <tui-loader textContent="Loading logs" />
        }
      </section>
      @for (log of logs(); track log) {
        <pre [innerHTML]="log | dompurify"></pre>
      }
    </tui-scrollbar>
  `,
  styles: `
    :host {
      max-height: 100dvh;
      display: flex;
      flex-direction: column;
      justify-content: flex-start;
      padding: 1rem;
      gap: 1rem;
      background: var(--tui-background-base);
    }

    pre {
      white-space: normal;
    }
  `,
  imports: [
    RouterLink,
    WaIntersectionObserver,
    WaMutationObserver,
    NgDompurifyPipe,
    TuiButton,
    TuiLoader,
    TuiScrollbar,
  ],
  providers: [tuiProvide(WA_INTERSECTION_ROOT, ElementRef)],
})
export default class LogsPage implements OnInit {
  @ViewChild(TuiScrollbar, { read: ElementRef })
  private readonly scrollbar?: ElementRef<HTMLElement>
  private readonly api = inject(ApiService)
  private readonly errorService = inject(ErrorService)

  startCursor?: string | null
  readonly loading = signal(false)
  readonly logs = signal<string[]>([])
  scrollTop = 0

  ngOnInit() {
    this.getLogs()
  }

  onTop(top: boolean) {
    if (top) this.getLogs()
  }

  restoreScroll() {
    if (this.loading() || !this.scrollbar) return

    const scrollbar = this.scrollbar.nativeElement
    const offset = scrollbar.querySelector('pre')?.clientHeight || 0

    scrollbar.scrollTop = this.scrollTop + offset
  }

  private async getLogs() {
    if (this.loading()) return

    this.loading.set(true)

    try {
      const response = await this.api.rpc.diagnostic.logs({
        cursor: this.startCursor ?? undefined,
        before: !!this.startCursor,
        limit: 200,
      })

      if (!response.entries.length) return

      this.startCursor = response.startCursor
      this.logs.update(logs => [convertAnsi(response.entries), ...logs])
      this.scrollTop = this.scrollbar?.nativeElement.scrollTop || 0
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      this.loading.set(false)
    }
  }
}
