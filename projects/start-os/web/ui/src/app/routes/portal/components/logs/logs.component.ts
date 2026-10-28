import { CommonModule } from '@angular/common'
import { Component, ElementRef, Input, ViewChild } from '@angular/core'
import {
  WA_INTERSECTION_ROOT,
  WaIntersectionObserver,
} from '@ng-web-apis/intersection-observer'
import { WaMutationObserver } from '@ng-web-apis/mutation-observer'
import { i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { tuiProvide } from '@taiga-ui/cdk'
import { TuiButton, TuiLoader, TuiScrollbar } from '@taiga-ui/core'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { BehaviorSubject } from 'rxjs'
import { FollowServerLogsReq } from 'src/app/services/api/api.types'
import { LogsDownloadDirective } from './logs-download.directive'
import { LogsFetchDirective } from './logs-fetch.directive'
import { LogsPipe } from './logs.pipe'

@Component({
  selector: 'logs',
  template: `
    <tui-scrollbar class="scrollbar">
      <section
        class="top"
        waIntersectionObserver
        (waIntersectionObservee)="onLoading(!!$event[0]?.isIntersecting)"
        (logsFetch)="onPrevious($event)"
      >
        @if (loading) {
          <tui-loader [textContent]="'Loading older logs' | i18n" />
        }
      </section>

      <section #el childList (waMutationObserver)="restoreScroll(el)">
        @for (log of previous; track log) {
          <pre [innerHTML]="log | dompurify"></pre>
        }
      </section>

      @if (followLogs | logs | async; as logs) {
        <section childList (waMutationObserver)="scrollToBottom()">
          @for (log of logs; track $index) {
            <pre [innerHTML]="log | dompurify"></pre>
          }

          @if ((status$ | async) !== 'connected') {
            <div class="g-dots" [attr.data-status]="status$.value">
              {{
                status$.value === 'reconnecting'
                  ? ('Reconnecting' | i18n)
                  : ('Waiting for network connectivity' | i18n)
              }}
            </div>
          }
        </section>
      } @else {
        <tui-loader class="loader" [textContent]="'Loading logs' | i18n" />
      }

      <section
        #bottom
        class="bottom"
        waIntersectionObserver
        (waIntersectionObservee)="
          setScroll(!!$event[$event.length - 1]?.isIntersecting)
        "
      ></section>
    </tui-scrollbar>

    <footer class="footer">
      <button
        tuiButton
        appearance="flat-grayscale"
        iconStart="@tui.circle-arrow-down"
        (click)="setScroll(true); scrollToBottom()"
      >
        {{ 'Scroll to bottom' | i18n }}
      </button>
      <button
        tuiButton
        appearance="flat-grayscale"
        iconStart="@tui.download"
        [logsDownload]="fetchLogs"
      >
        {{ 'Download' | i18n }}
      </button>
    </footer>
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      height: 100%;
      overflow: hidden;
    }

    .scrollbar {
      flex: 1;
      background: var(--tui-background-neutral-1);
      border-radius: var(--tui-radius-m);
      border: 1rem solid transparent;
    }

    .g-dots {
      text-align: center;
    }

    .top {
      height: 10rem;
      margin-bottom: -5rem;
    }

    .loader {
      position: absolute;
      inset: 0;
    }

    .bottom {
      height: 3rem;
    }

    .footer {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding-top: 1rem;
    }

    [data-status='reconnecting'] {
      color: var(--tui-status-positive);
    }

    [data-status='disconnected'] {
      color: var(--tui-status-warning);
    }

    pre {
      overflow: visible;
      white-space: normal;
      overflow-wrap: anywhere;
      margin: 0;
    }
  `,
  imports: [
    CommonModule,
    WaIntersectionObserver,
    WaMutationObserver,
    NgDompurifyPipe,
    TuiButton,
    TuiLoader,
    TuiScrollbar,
    LogsDownloadDirective,
    LogsFetchDirective,
    LogsPipe,
    i18nPipe,
  ],
  providers: [tuiProvide(WA_INTERSECTION_ROOT, ElementRef)],
})
export class LogsComponent {
  @ViewChild('bottom')
  private readonly bottom?: ElementRef<HTMLElement>

  @ViewChild(TuiScrollbar, { read: ElementRef })
  private readonly scrollbar?: ElementRef<HTMLElement>

  @Input({ required: true }) followLogs!: (
    params: FollowServerLogsReq,
  ) => Promise<T.LogFollowResponse>

  @Input({ required: true }) fetchLogs!: (
    params: T.LogsParamsInput,
  ) => Promise<T.LogResponse>

  @Input({ required: true }) context!: string

  scrollTop = 0
  startCursor?: string
  scroll = true
  loading = false
  previous: readonly string[] = []

  readonly status$ = new BehaviorSubject<
    'connected' | 'disconnected' | 'reconnecting'
  >('connected')

  onLoading(loading: boolean) {
    this.loading = loading && !this.scroll
  }

  onPrevious(previous: string) {
    this.onLoading(false)
    this.scrollTop = this.scrollbar?.nativeElement.scrollTop || 0
    this.previous = [previous, ...this.previous]
  }

  setCursor(startCursor = this.startCursor) {
    this.startCursor = startCursor
  }

  setScroll(scroll: boolean) {
    this.scroll = scroll
  }

  restoreScroll({ firstElementChild }: HTMLElement) {
    this.scrollbar?.nativeElement.scrollTo(
      this.scrollbar?.nativeElement.scrollLeft || 0,
      this.scrollTop + (firstElementChild?.clientHeight || 0),
    )
  }

  scrollToBottom() {
    if (this.scroll)
      this.bottom?.nativeElement.scrollIntoView({
        behavior: 'smooth',
      })
  }
}
