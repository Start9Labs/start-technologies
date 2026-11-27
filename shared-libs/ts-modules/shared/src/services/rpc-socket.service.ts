import { DOCUMENT, inject, Service } from '@angular/core'
import type { RpcStreamOptions } from '@start9labs/start-core'
import { SESSION_PROFILE } from '@start9labs/start-core/osBindings/session-profile'
import {
  catchError,
  defer,
  filter,
  firstValueFrom,
  map,
  Observable,
  ReplaySubject,
  Subscription,
  take,
  tap,
  throwError,
} from 'rxjs'
import { webSocket } from 'rxjs/webSocket'
import { v4 as uuid } from 'uuid'
import type { WebSocketSubject, WebSocketSubjectConfig } from 'rxjs/webSocket'

import { RELATIVE_URL } from '../tokens/relative-url'
import type { RPCOptions, RPCResponse } from '../types/rpc.types'
import { AUTH_SIGNING_HOST, AuthKeyService } from './auth-key.service'
import type { SessionSocketIdentity } from './session-websocket'
import { withTimeout } from './http.service'
import {
  DeviceClockError,
  SessionOpenEvent,
  assertServerIdentity,
  sessionWebSocketConstructor,
} from './session-websocket'
import { RpcConsoleLog } from './rpc-console-log'
import { rpcWebSocketUrl } from './rpc-request'
import { isRpcResponse, rpcResponseStream } from './rpc-stream'
import type { RpcFrame, SocketResponseStream } from './rpc-stream'

/** Observes verified admission for this operation, including healthy connection reuse. */
export type RpcSocketOptions = Pick<RPCOptions, 'timeout'> &
  Pick<WebSocketSubjectConfig<unknown>, 'openObserver'> & {
    allowTemporaryIdentity?: boolean
  }

@Service()
export class RpcSocketService {
  private readonly document = inject(DOCUMENT)
  private readonly relativeUrl = inject(RELATIVE_URL)
  private readonly context = inject(AUTH_SIGNING_HOST)
  private readonly authKeys = inject(AuthKeyService)
  private readonly consoleLog = inject(RpcConsoleLog)
  private session?: {
    signal: AbortSignal
    abort: AbortController
    socket: WebSocketSubject<RpcFrame>
    opened: ReplaySubject<SessionOpenEvent>
  }

  /** Adds a correlation ID to an ID-less serialized RPC object. */
  async message<T>(
    body: string,
    options: RpcSocketOptions,
    lease: SessionSocketIdentity,
  ): Promise<RPCResponse<T>> {
    let response = this.responses(body, lease, options).pipe(
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
    lease: SessionSocketIdentity,
    options: RpcStreamOptions & RpcSocketOptions = {},
  ): Promise<SocketResponseStream<R, I, F>> {
    const request = `{"rpcstream":${JSON.stringify(SESSION_PROFILE.transport.rpcStreamVersion)},${body.slice(1)}`
    let source = this.responses(request, lease, options)
    if (options.timeout)
      source = withTimeout(source, options.timeout).pipe(
        catchError(cause => throwError(() => transportError(cause))),
      )
    return rpcResponseStream(source, options)
  }

  private responses(
    body: string,
    lease: SessionSocketIdentity,
    options: RpcSocketOptions,
  ): Observable<RpcFrame> {
    return defer(() => {
      const { socket, opened } = this.sessionFor(lease)
      const id = uuid()
      const request = `{"jsonrpc":"2.0","id":${JSON.stringify(id)},${body.slice(1)}`
      const responses = socket.multiplex(
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
      const logged = this.consoleLog.request(id, request)
      return new Observable<RpcFrame>(subscriber => {
        const subscription = new Subscription()
        subscription.add(
          opened
            .pipe(
              filter(
                event =>
                  (event.target as WebSocket).readyState === WebSocket.OPEN,
              ),
              take(1),
            )
            .subscribe(event => {
              try {
                assertServerIdentity(
                  event,
                  options.allowTemporaryIdentity ?? false,
                )
              } catch (error) {
                subscriber.error(error)
                return
              }
              options.openObserver?.next(event)
              subscription.add(responses.subscribe(subscriber))
            }),
        )
        if (!subscriber.closed)
          subscription.add(
            socket.subscribe({
              error: error => subscriber.error(error),
              complete: () => subscriber.complete(),
            }),
          )
        return subscription
      }).pipe(
        tap({
          next: frame => logged.reply(frame.value),
          error: error => logged.failure(error),
        }),
      )
    }).pipe(catchError(cause => throwError(() => transportError(cause))))
  }

  private sessionFor(lease: SessionSocketIdentity) {
    lease.signal.throwIfAborted()
    if (this.session?.signal !== lease.signal) {
      const abort = new AbortController()
      const signal = AbortSignal.any([lease.signal, abort.signal])
      const opened = new ReplaySubject<SessionOpenEvent>(1)
      const socket = webSocket<RpcFrame>({
        url: this.url(),
        openObserver: { next: event => opened.next(event as SessionOpenEvent) },
        WebSocketCtor: sessionWebSocketConstructor(
          async () => ({ signer: lease.signer, signal }),
          this.context,
          SESSION_PROFILE.identity.rpcTarget,
          undefined,
          true,
        ),
        serializer: value =>
          typeof value === 'string' ? value : JSON.stringify(value),
        deserializer: event => ({
          value: JSON.parse(event.data),
          bytes: new TextEncoder().encode(event.data).byteLength,
        }),
      })
      this.session = { signal: lease.signal, abort, socket, opened }
      lease.signal.addEventListener(
        'abort',
        () => {
          if (this.session?.signal === lease.signal) this.session = undefined
        },
        { once: true, signal: abort.signal },
      )
    }
    return this.session
  }

  /** Aborts active RPC calls; the next call opens a fresh session. */
  disconnect(): void {
    const session = this.session
    this.session = undefined
    session?.abort.abort(new Error('RPC session disconnected'))
  }

  /** Opens an encrypted continuation with the DOM event/lifetime contract. */
  openSocket(guid: string): WebSocket {
    const Constructor = this.constructorFor(guid)
    return new Constructor(this.url(guid))
  }

  /** Redeems a continuation through encrypted admission. */
  open<T>(
    guid: string,
    {
      allowTemporaryIdentity = false,
      ...config
    }: Omit<WebSocketSubjectConfig<T>, 'url' | 'WebSocketCtor'> &
      Pick<RpcSocketOptions, 'allowTemporaryIdentity'> = {},
  ): WebSocketSubject<T> {
    return webSocket<T>({
      ...config,
      url: this.url(guid),
      WebSocketCtor: this.constructorFor(guid, allowTemporaryIdentity),
    })
  }

  private url(guid?: string): string {
    return rpcWebSocketUrl(
      this.relativeUrl,
      this.document.location.origin,
      guid,
    )
  }

  private constructorFor(guid: string, allowTemporaryIdentity = false) {
    return sessionWebSocketConstructor(
      () => this.authKeys.admissionLease(),
      this.context,
      { continuation: guid },
      undefined,
      allowTemporaryIdentity,
    )
  }
}

function deviceClockError(cause: unknown): DeviceClockError | undefined {
  for (let value = cause; value !== undefined;) {
    if (value instanceof DeviceClockError) return value
    value =
      value instanceof Event
        ? (value as ErrorEvent).error
        : value instanceof Error
          ? value.cause
          : undefined
  }
  return undefined
}

function transportError(cause: unknown) {
  const clock = deviceClockError(cause)
  return Object.assign(
    new Error(clock?.message ?? 'Encrypted RPC transport failed', { cause }),
    { code: 0 },
  )
}
