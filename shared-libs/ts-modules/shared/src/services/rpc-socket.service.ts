import { DOCUMENT, inject, Service } from '@angular/core'
import { auth } from '@start9labs/start-core'
import type { RpcStreamOptions } from '@start9labs/start-core'
import { SESSION_PROFILE } from '@start9labs/start-core/osBindings/session-profile'
import {
  catchError,
  defer,
  filter,
  firstValueFrom,
  map,
  throwError,
} from 'rxjs'
import type { Observable } from 'rxjs'
import { webSocket } from 'rxjs/webSocket'
import type { WebSocketSubject, WebSocketSubjectConfig } from 'rxjs/webSocket'

import { RELATIVE_URL } from '../tokens/relative-url'
import type { RPCOptions, RPCResponse } from '../types/rpc.types'
import { AUTH_SIGNING_HOST, AuthKeyService } from './auth-key.service'
import type { AuthKeyLease } from './auth-key.service'
import { withTimeout } from './http.service'
import {
  SessionOpenEvent,
  sessionWebSocketConstructor,
} from './session-websocket'
import { ServerIdentityService } from './server-identity.service'
import { rpcWebSocketUrl } from './rpc-request'
import { isRpcResponse, rpcResponseStream } from './rpc-stream'
import type { RpcFrame, SocketResponseStream } from './rpc-stream'

@Service()
export class RpcSocketService {
  private readonly document = inject(DOCUMENT)
  private readonly relativeUrl = inject(RELATIVE_URL)
  private readonly context = inject(AUTH_SIGNING_HOST)
  private readonly authKeys = inject(AuthKeyService)
  private readonly serverIdentity = inject(ServerIdentityService)
  private readonly opened = (event: Event) => {
    void this.serverIdentity.observe((event as SessionOpenEvent).peer)
  }
  private session?: {
    signal: AbortSignal
    abort: AbortController
    socket: WebSocketSubject<RpcFrame>
  }

  /** Adds a correlation ID to an ID-less serialized RPC object. */
  async message<T>(
    body: string,
    options: Pick<RPCOptions, 'timeout'>,
    lease: AuthKeyLease,
  ): Promise<RPCResponse<T>> {
    let response = this.responses(body, lease).pipe(
      map(frame => frame.value),
      filter(
        (value): value is RPCResponse<T> =>
          isRpcResponse(value) && 'jsonrpc' in value,
      ),
    )
    if (options.timeout) response = withTimeout(response, options.timeout)
    try {
      return await firstValueFrom(response)
    } catch (cause) {
      throw transportError(cause)
    }
  }

  /** Cancellation includes admission and initialization; terminal values belong to the iterator. */
  stream<R, I, F>(
    body: string,
    lease: AuthKeyLease,
    options: RpcStreamOptions = {},
  ): Promise<SocketResponseStream<R, I, F>> {
    const request = `{"rpcstream":${JSON.stringify(SESSION_PROFILE.transport.rpcStreamVersion)},${body.slice(1)}`
    let source = this.responses(request, lease)
    if (options.timeout)
      source = withTimeout(source, options.timeout).pipe(
        catchError(cause => throwError(() => transportError(cause))),
      )
    return rpcResponseStream(source, options)
  }

  private responses(body: string, lease: AuthKeyLease): Observable<RpcFrame> {
    return defer(() => {
      const socket = this.socket(lease)
      const id = this.document.defaultView!.crypto.randomUUID()
      const request = `{"jsonrpc":"2.0","id":${JSON.stringify(id)},${body.slice(1)}`
      return socket.multiplex(
        () => request,
        () => ({
          rpcstream: SESSION_PROFILE.transport.rpcStreamVersion,
          close: id,
        }),
        frame =>
          frame.value !== null &&
          typeof frame.value === 'object' &&
          'id' in frame.value &&
          frame.value.id === id,
      )
    }).pipe(catchError(cause => throwError(() => transportError(cause))))
  }

  private socket(lease: AuthKeyLease): WebSocketSubject<RpcFrame> {
    const prepared = this.prepare(lease)
    lease.signal.throwIfAborted()
    if (this.session?.signal !== lease.signal) {
      const abort = new AbortController()
      const signal = AbortSignal.any([prepared.signal, abort.signal])
      const socket = webSocket<RpcFrame>({
        url: this.url(),
        openObserver: { next: this.opened },
        WebSocketCtor: sessionWebSocketConstructor(
          async () => ({ ...prepared, signal }),
          this.context,
          SESSION_PROFILE.identity.rpcTarget,
        ),
        serializer: value =>
          typeof value === 'string' ? value : JSON.stringify(value),
        deserializer: event => ({
          value: JSON.parse(event.data),
          bytes: new TextEncoder().encode(event.data).byteLength,
        }),
      })
      this.session = { signal: lease.signal, abort, socket }
      lease.signal.addEventListener(
        'abort',
        () => {
          if (this.session?.signal === lease.signal) this.session = undefined
        },
        { once: true, signal: abort.signal },
      )
    }
    return this.session.socket
  }

  /** Aborts active RPC calls; the next call opens a fresh session. */
  disconnect(): void {
    const session = this.session
    this.session = undefined
    session?.abort.abort(new Error('RPC session disconnected'))
  }

  /** Opens an encrypted continuation with the DOM event/lifetime contract. */
  openSocket(guid: string): WebSocket {
    const Constructor = this.constructorFor(guid, () => this.authKeys.lease())
    const socket = new Constructor(this.url(guid))
    socket.addEventListener('open', this.opened, { once: true })
    return socket
  }

  /** Redeems a continuation through encrypted admission. */
  open<T>(
    guid: string,
    config: Omit<WebSocketSubjectConfig<T>, 'url' | 'WebSocketCtor'> = {},
  ): WebSocketSubject<T> {
    return webSocket<T>({
      ...config,
      openObserver: {
        next: event => {
          this.opened(event)
          config.openObserver?.next(event)
        },
      },
      url: this.url(guid),
      WebSocketCtor: this.constructorFor(guid, () => this.authKeys.lease()),
    })
  }

  private url(guid?: string): string {
    return rpcWebSocketUrl(
      this.relativeUrl,
      this.document.location.origin,
      guid,
    )
  }

  private constructorFor(
    guid: string,
    lease: () => Promise<AuthKeyLease | null>,
  ) {
    return sessionWebSocketConstructor(
      async () => {
        const current = await lease()
        if (!current) throw new Error('No active device identity')
        return this.prepare(current)
      },
      this.context,
      { continuation: guid },
    )
  }

  private prepare(current: AuthKeyLease) {
    current.signal.throwIfAborted()
    return { signer: auth.requestSigner(current.key), signal: current.signal }
  }
}

function transportError(cause: unknown) {
  return Object.assign(new Error('Encrypted RPC transport failed', { cause }), {
    code: 0,
  })
}
