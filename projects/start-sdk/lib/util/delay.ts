const MAX_TIMEOUT_MS = 2 ** 31 - 1

/**
 * Resolves after `ms`, however long, or as soon as the abort signal fires.
 * Never resolves for `Infinity` without an abort.
 */
export function delay(ms: number, abort?: AbortSignal): Promise<void> {
  if (abort?.aborted) return Promise.resolve()
  return new Promise<void>(resolve => {
    let timer: ReturnType<typeof setTimeout> | undefined
    const wake = () => {
      clearTimeout(timer)
      abort?.removeEventListener('abort', wake)
      resolve()
    }
    const wait = (remaining: number) => {
      if (remaining === Infinity) return
      const step = Math.min(remaining, MAX_TIMEOUT_MS)
      timer = setTimeout(
        () => (remaining > step ? wait(remaining - step) : wake()),
        step,
      )
    }
    abort?.addEventListener('abort', wake)
    wait(ms)
  })
}

/** Delays that double from the initial delay up to the cap. */
export class ExponentialBackoff {
  private nextMs: number

  constructor(
    private readonly initialMs: number,
    private readonly maxMs: number,
  ) {
    this.nextMs = initialMs
  }

  /** Returns the current delay and doubles the one after it. */
  next(): number {
    const ms = this.nextMs
    this.nextMs = Math.min(ms * 2, this.maxMs)
    return ms
  }

  reset() {
    this.nextMs = this.initialMs
  }
}
