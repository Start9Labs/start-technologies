import type {
  RpcResponseStream,
  RpcStreamOptions,
} from '@start9labs/start-core'
import { SESSION_PROFILE } from '@start9labs/start-core/osBindings/session-profile'
import { Subscription } from 'rxjs'
import type { Observable } from 'rxjs'
import { RpcError } from '../classes/rpc-error'
import type {
  RPCResponse,
  RPCSuccessRes,
  RPCErrorRes,
} from '../types/rpc.types'
import { isRpcError } from '../types/rpc.types'

type StreamResponse = (
  | Omit<RPCSuccessRes<unknown>, 'jsonrpc'>
  | Omit<RPCErrorRes, 'jsonrpc'>
) & {
  rpcstream: typeof SESSION_PROFILE.transport.rpcStreamVersion
  end: boolean
}

export type RpcFrame = {
  value: unknown
  /** Complete encoded UTF-8 message length. */
  bytes: number
}
export type SocketResponseStream<R, I, F> = RpcResponseStream<R, I, F> & {
  meta?: RPCResponse<R>['meta']
}

export function isRpcResponse(
  value: unknown,
): value is RPCResponse<unknown> | StreamResponse {
  if (
    value === null ||
    typeof value !== 'object' ||
    'result' in value === 'error' in value
  )
    return false
  if ('rpcstream' in value)
    return (
      value.rpcstream === SESSION_PROFILE.transport.rpcStreamVersion &&
      'end' in value &&
      typeof value.end === 'boolean'
    )
  return 'jsonrpc' in value && value.jsonrpc === '2.0'
}

/**
 * Bounds queued UTF-8 bytes while initiation fences or consumers pause.
 * Terminals drain accepted items; transport loss discards them.
 */
export async function rpcResponseStream<R, I, F>(
  source: Observable<RpcFrame>,
  options: RpcStreamOptions = {},
): Promise<SocketResponseStream<R, I, F>> {
  options.signal?.throwIfAborted()
  const subscription = new Subscription()
  let resolveInitial!: (value: R) => void
  let rejectInitial!: (error: unknown) => void
  const initial = new Promise<R>((resolve, reject) => {
    resolveInitial = resolve
    rejectInitial = reject
  })
  let initializing = true
  let meta: RPCResponse<R>['meta']
  let terminal: { result: F } | { error: unknown } | undefined
  let queuedBytes = 0
  let controller!: ReadableStreamDefaultController<RpcFrame>
  const readable = new ReadableStream<RpcFrame>({
    start(value) {
      controller = value
    },
  })
  const detach = () => {
    subscription.unsubscribe()
    options.signal?.removeEventListener('abort', aborted)
  }
  const fail = (error: unknown) => {
    if (initializing) {
      initializing = false
      rejectInitial(error)
    }
    queuedBytes = 0
    controller.error(error)
    detach()
  }
  const aborted = () => fail(options.signal!.reason)
  options.signal?.addEventListener('abort', aborted, { once: true })
  subscription.add(
    source.subscribe({
      next(frame) {
        const value = frame.value
        if (!isRpcResponse(value)) {
          fail(new Error('Invalid RPC response'))
          return
        }
        if (initializing) {
          if (!('jsonrpc' in value)) {
            fail(
              new Error('Response stream started without an initiating reply'),
            )
            return
          }
          if (isRpcError(value)) {
            fail(new RpcError(value.error, value.meta))
            return
          }
          initializing = false
          meta = value.meta
          resolveInitial(value.result as R)
          return
        }
        if (!('rpcstream' in value)) {
          fail(new Error('Unexpected initiating reply'))
          return
        }
        if (value.end) {
          terminal = isRpcError(value)
            ? { error: new RpcError(value.error) }
            : { result: value.result as F }
          controller.close()
          subscription.unsubscribe()
        } else if (isRpcError(value)) {
          fail(new RpcError(value.error))
        } else if (
          queuedBytes + frame.bytes >
          SESSION_PROFILE.transport.maxIncomingBytes
        ) {
          fail(
            new RpcError({
              code: SESSION_PROFILE.transport.rpcStreamCancelledCode,
              message: 'Response stream buffer exceeded',
              data: null,
            }),
          )
        } else {
          queuedBytes += frame.bytes
          controller.enqueue(frame)
        }
      },
      error: fail,
      complete() {
        if (!terminal)
          fail(
            Object.assign(new Error('Response stream lost its transport'), {
              code: 0,
            }),
          )
      },
    }),
  )
  const value = await initial
  const reader = readable.getReader()
  const items = (async function* (): AsyncGenerator<I, F, void> {
    try {
      while (true) {
        const frame = await reader.read()
        if (frame.done) {
          if (!terminal)
            throw new Error('Response stream ended without a terminal result')
          if ('error' in terminal) throw terminal.error
          return terminal.result
        }
        queuedBytes -= frame.value.bytes
        yield (frame.value.value as { result: I }).result
      }
    } finally {
      detach()
      reader.releaseLock()
    }
  })()
  const finish = items.return.bind(items)
  const reject = items.throw.bind(items)
  // Generator disposal must work before its first next().
  items.return = result => {
    fail(new DOMException('Response stream disposed', 'AbortError'))
    reader.releaseLock()
    return finish(result)
  }
  items.throw = error => {
    fail(error)
    reader.releaseLock()
    return reject(error)
  }
  return { initial: value, meta, items }
}
