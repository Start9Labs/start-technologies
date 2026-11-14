import { inject, Service } from '@angular/core'
import { WA_WINDOW } from '@ng-web-apis/common'

import { isRpcError } from '../types/rpc.types'
import type { RPCRequest } from '../types/rpc.types'

const STORAGE_KEY = 'rpcConsoleLog'

/** Window property through which a developer toggles logging from the browser console. */
export const RPC_CONSOLE_LOG_GLOBAL = 'rpcLog'

export interface RpcConsoleExchange {
  reply(value: unknown): void
  failure(error: unknown): void
}

/**
 * Prints decrypted session RPC traffic to the browser console while enabled.
 * Enablement persists in this browser's local storage until disabled.
 */
@Service()
export class RpcConsoleLog {
  private readonly window = inject(WA_WINDOW)
  private readonly storage = this.window.localStorage
  private on = this.storage.getItem(STORAGE_KEY) === 'true'

  constructor() {
    Object.assign(this.window, { [RPC_CONSOLE_LOG_GLOBAL]: this })
  }

  get enabled(): boolean {
    return this.on
  }

  enable(): void {
    this.on = true
    this.storage.setItem(STORAGE_KEY, 'true')
    console.log('RPC console logging enabled')
  }

  disable(): void {
    this.on = false
    this.storage.removeItem(STORAGE_KEY)
    console.log('RPC console logging disabled')
  }

  /** Logs the request and returns its reply logger; null while disabled. */
  request(id: string, serialized: string): RpcConsoleExchange | null {
    if (!this.on) return null
    const { method, params }: RPCRequest<unknown> = JSON.parse(serialized)
    const label = `RPC ${id.slice(0, 8)} ${method}`
    const started = performance.now()
    const elapsed = () => `${Math.round(performance.now() - started)} ms`
    console.log(`${label} →`, params)
    return {
      reply: value =>
        console[isErrorReply(value) ? 'warn' : 'log'](
          `${label} ← ${elapsed()}`,
          value,
        ),
      failure: error =>
        console.warn(`${label} transport failed ${elapsed()}`, error),
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
