const MAX_TIMEOUT_MS = 2 ** 31 - 1

export type DelayOptions = {
  signal?: AbortSignal
  ref?: boolean
}

/**
 * Resolves after `ms`, however long, or as soon as the signal aborts.
 * Never resolves for `Infinity` without a signal.
 */
export function delay(
  ms: number,
  { signal, ref = true }: DelayOptions = {},
): Promise<void> {
  if (signal?.aborted) return Promise.resolve()
  return new Promise<void>(resolve => {
    let timer: ReturnType<typeof setTimeout> | undefined
    const wake = () => {
      clearTimeout(timer)
      signal?.removeEventListener('abort', wake)
      resolve()
    }
    const wait = (remaining: number) => {
      if (remaining === Infinity) return
      const step = Math.min(remaining, MAX_TIMEOUT_MS)
      timer = setTimeout(
        () => (remaining > step ? wait(remaining - step) : wake()),
        step,
      )
      if (!ref) timer.unref()
    }
    signal?.addEventListener('abort', wake)
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
