import { inject, Service } from '@angular/core'
import { WA_WINDOW } from '@ng-web-apis/common'

import { isRpcError } from '../types/rpc.types'
import type { RPCRequest } from '../types/rpc.types'

const STORAGE_KEY = 'rpcConsoleLogUntil'
const DEFAULT_MINUTES = 30

/** Window property through which a developer toggles logging from the browser console. */
export const RPC_CONSOLE_LOG_GLOBAL = 'rpcLog'

export interface RpcConsoleExchange {
  reply(value: unknown): void
  failure(error: unknown): void
}

/**
 * Prints decrypted session RPC traffic to the browser console while enabled.
 * Enablement persists in this browser's local storage until it expires or is disabled.
 * The live toggle governs active calls and streams.
 */
@Service()
export class RpcConsoleLog {
  private readonly window = inject(WA_WINDOW)
  private readonly storage = this.window.localStorage
  private expiry = Number(this.storage.getItem(STORAGE_KEY))

  constructor() {
    Object.assign(this.window, { [RPC_CONSOLE_LOG_GLOBAL]: this })
  }

  get enabled(): boolean {
    return Date.now() < this.expiry
  }

  enable(minutes = DEFAULT_MINUTES): void {
    this.expiry = Date.now() + minutes * 60_000
    this.storage.setItem(STORAGE_KEY, String(this.expiry))
    console.log(
      `RPC console logging enabled until ${new Date(this.expiry).toLocaleString()}`,
    )
  }

  disable(): void {
    this.expiry = 0
    this.storage.removeItem(STORAGE_KEY)
    console.log('RPC console logging disabled')
  }

  /** Measures replies from request creation, including when initially disabled. */
  request(id: string, serialized: string): RpcConsoleExchange {
    const started = performance.now()
    const elapsed = () => `${Math.round(performance.now() - started)} ms`
    let request: RPCRequest<unknown> | undefined
    const label = () => {
      request ??= JSON.parse(serialized) as RPCRequest<unknown>
      return `RPC ${id.slice(0, 8)} ${request.method}`
    }
    if (this.enabled) console.log(`${label()} →`, request!.params)
    return {
      reply: value => {
        if (!this.enabled) return
        console[isErrorReply(value) ? 'warn' : 'log'](
          `${label()} ← ${elapsed()}`,
          value,
        )
      },
      failure: error => {
        if (!this.enabled) return
        console.warn(`${label()} transport failed ${elapsed()}`, error)
      },
    }
  }
}

function isErrorReply(value: unknown): boolean {
  return (
    value !== null &&
    typeof value === 'object' &&
    isRpcError(value as { error: unknown } | { result: unknown })
  )
}
