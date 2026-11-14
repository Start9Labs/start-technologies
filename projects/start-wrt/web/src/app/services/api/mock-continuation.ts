import type { ContinuationSocket, FullProgress } from './api.service'

type Handler<K extends Exclude<keyof ContinuationSocket, 'close'>> =
  OmitThisParameter<NonNullable<ContinuationSocket[K]>> | null

export class MockContinuation implements ContinuationSocket {
  onopen: Handler<'onopen'> = null
  onmessage: Handler<'onmessage'> = null
  onclose: Handler<'onclose'> = null
  onerror: Handler<'onerror'> = null
  private closed = false

  constructor(progress?: FullProgress) {
    queueMicrotask(() => {
      if (this.closed) return
      this.onopen?.(new Event('open'))
      if (progress && !this.closed) {
        this.onmessage?.(
          new MessageEvent('message', { data: JSON.stringify(progress) }),
        )
        this.close()
      }
    })
  }

  close(): void {
    if (this.closed) return
    this.closed = true
    queueMicrotask(() =>
      this.onclose?.(new CloseEvent('close', { code: 1000, wasClean: true })),
    )
  }
}
