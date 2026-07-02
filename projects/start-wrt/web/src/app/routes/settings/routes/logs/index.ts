import {
  Component,
  computed,
  DestroyRef,
  ElementRef,
  inject,
  signal,
  viewChild,
} from '@angular/core'
import { TuiButton } from '@taiga-ui/core'
import { ApiService, LogEntry } from 'src/app/services/api/api.service'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  template: `
    <pre #logPre>{{ entries().length ? logText() : status() }}</pre>
    <footer class="g-footer">
      <button
        tuiButton
        appearance="outline"
        iconStart="@tui.download"
        (click)="download()"
      >
        {{ 'Download' | i18n }}
      </button>
      <button
        tuiButton
        appearance="outline"
        iconStart="@tui.arrow-down"
        (click)="scrollToBottom()"
      >
        {{ 'Scroll to Bottom' | i18n }}
      </button>
    </footer>
  `,
  styles: `
    :host {
      max-width: 50rem;
      height: calc(100dvh - 11rem);
      display: flex;
      flex-direction: column;
      overflow: hidden;
    }

    pre {
      flex: 1;
      min-height: 0;
      padding: 1rem;
      margin: 1rem 0;
      overflow: auto;
      background: var(--tui-background-neutral-1);
      border-radius: var(--tui-radius-s);
      white-space: pre-wrap;
      word-break: break-all;
      font-size: 0.75rem;
      line-height: 1.4;
    }

    button:first-child {
      margin-inline-end: auto;
    }
  `,
  host: { class: 'g-page' },
  imports: [TuiButton, i18nPipe],
})
export default class Logs {
  private readonly api = inject(ApiService)
  private readonly destroyRef = inject(DestroyRef)
  private readonly i18n = inject(i18nPipe)
  private readonly logPre = viewChild<ElementRef<HTMLPreElement>>('logPre')

  readonly entries = signal<LogEntry[]>([])
  readonly status = signal(this.i18n.transform('Loading...'))
  readonly logText = computed(() =>
    this.entries()
      .map(e => `${e.timestamp} ${e.message}`)
      .join('\n'),
  )

  private ws: WebSocket | null = null
  private shouldAutoScroll = true
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null
  private reconnectAttempts = 0
  private destroyed = false

  constructor() {
    // Open the live stream first (synchronously, before any await) so a quick
    // destroy/recreate cycle doesn't lose the socket. The snapshot RPC is
    // best-effort historical context for what happened before this view.
    this.connectWs()
    this.loadInitial()
    this.destroyRef.onDestroy(() => {
      this.destroyed = true
      this.ws?.close()
      if (this.reconnectTimer) clearTimeout(this.reconnectTimer)
    })
  }

  private async loadInitial() {
    try {
      const res = await this.api.systemLogs()
      this.entries.set(res.entries)
      requestAnimationFrame(() => this.scrollToBottom())
    } catch {
      this.status.set(this.i18n.transform('Failed to load logs'))
    }
  }

  private connectWs() {
    if (this.destroyed) return
    const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:'
    const ws = new WebSocket(`${protocol}//${location.host}/api/logs`)
    this.ws = ws

    ws.onopen = () => {
      this.reconnectAttempts = 0
    }

    ws.onmessage = event => {
      const entry: LogEntry = JSON.parse(event.data)
      this.checkAutoScroll()
      this.entries.update(entries => [...entries, entry])
      if (this.shouldAutoScroll) {
        requestAnimationFrame(() => this.scrollToBottom())
      }
    }

    ws.onclose = () => {
      if (this.destroyed) return
      if (this.reconnectAttempts < 5) {
        this.reconnectAttempts++
        this.reconnectTimer = setTimeout(() => this.connectWs(), 3000)
      }
    }
  }

  private checkAutoScroll() {
    const el = this.logPre()?.nativeElement
    if (!el) return
    this.shouldAutoScroll =
      el.scrollHeight - el.scrollTop - el.clientHeight < 50
  }

  scrollToBottom() {
    const el = this.logPre()?.nativeElement
    if (el) {
      el.scrollTop = el.scrollHeight
    }
  }

  async download() {
    let text: string
    try {
      const res = await this.api.systemLogs()
      text = res.entries.map(e => `${e.timestamp} ${e.message}`).join('\n')
    } catch {
      text = this.entries()
        .map(e => `${e.timestamp} ${e.message}`)
        .join('\n')
    }
    const blob = new Blob([text], { type: 'text/plain' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `logs-${new Date().toISOString().slice(0, 19)}.txt`
    a.click()
    URL.revokeObjectURL(url)
  }
}
