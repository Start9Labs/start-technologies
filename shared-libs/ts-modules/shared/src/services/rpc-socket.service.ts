import { DOCUMENT, inject, Service } from '@angular/core'
import { filter, firstValueFrom } from 'rxjs'
import { webSocket } from 'rxjs/webSocket'
import type { WebSocketSubject, WebSocketSubjectConfig } from 'rxjs/webSocket'

import { RELATIVE_URL } from '../tokens/relative-url'
import type { RPCOptions, RPCResponse } from '../types/rpc.types'
import { AUTH_SIGNING_HOST, AuthKeyService } from './auth-key.service'
import type { AuthKeyLease } from './auth-key.service'
import { withTimeout } from './http.service'
import { prepareNoiseIdentity } from './noise-identity'
import type { PreparedNoiseIdentity } from './noise-identity'
import { NoiseOpenEvent, noiseWebSocketConstructor } from './noise-websocket'
import { ServerIdentityService } from './server-identity.service'
import { rpcWebSocketUrl } from './rpc-request'

@Service()
export class RpcSocketService {
  private readonly document = inject(DOCUMENT)
  private readonly relativeUrl = inject(RELATIVE_URL)
  private readonly context = inject(AUTH_SIGNING_HOST)
  private readonly authKeys = inject(AuthKeyService)
  private readonly serverIdentity = inject(ServerIdentityService)
  private readonly opened = (event: Event) => {
    void this.serverIdentity.observe((event as NoiseOpenEvent).peer)
  }
  private readonly identities = new WeakMap<
    AbortSignal,
    Promise<PreparedNoiseIdentity>
  >()
  private session?: {
    signal: AbortSignal
    abort: AbortController
    socket: WebSocketSubject<unknown>
  }

  /** Adds a correlation ID to an ID-less serialized RPC object. */
  async message<T>(
    body: string,
    options: Pick<RPCOptions, 'timeout'>,
    lease: AuthKeyLease,
  ): Promise<RPCResponse<T>> {
    const prepared = await this.prepare(lease)
    lease.signal.throwIfAborted()
    if (this.session?.signal !== lease.signal) {
      const abort = new AbortController()
      const signal = AbortSignal.any([prepared.signal, abort.signal])
      const socket = webSocket<unknown>({
        url: this.url(),
        openObserver: { next: this.opened },
        WebSocketCtor: noiseWebSocketConstructor(
          async () => ({ ...prepared, signal }),
          this.context,
        ),
        serializer: value =>
          typeof value === 'string' ? value : JSON.stringify(value),
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
    const id = this.document.defaultView!.crypto.randomUUID()
    const request = `{"jsonrpc":"2.0","id":${JSON.stringify(id)},${body.slice(1)}`
    let response = this.session.socket
      .multiplex(
        () => request,
        () => ({ rpcstream: '1.0', close: id }),
        value =>
          value !== null &&
          typeof value === 'object' &&
          'id' in value &&
          value.id === id,
      )
      .pipe(
        filter(
          (value): value is RPCResponse<T> =>
            value !== null &&
            typeof value === 'object' &&
            ('result' in value || 'error' in value),
        ),
      )
    if (options.timeout) response = withTimeout(response, options.timeout)
    try {
      return await firstValueFrom(response)
    } catch (cause) {
      throw Object.assign(
        new Error('Encrypted RPC transport failed', { cause }),
        { code: 0 },
      )
    }
  }

  /** Aborts active RPC calls; the next call opens a fresh session. */
  disconnect(): void {
    const session = this.session
    this.session = undefined
    session?.abort.abort(new Error('RPC session disconnected'))
  }

  /** Opens an encrypted continuation with the DOM event/lifetime contract. */
  openSocket(guid: string): WebSocket {
    const Constructor = this.constructorFor(() => this.authKeys.lease())
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
      WebSocketCtor: this.constructorFor(() => this.authKeys.lease()),
    })
  }

  private url(guid?: string): string {
    return rpcWebSocketUrl(
      this.relativeUrl,
      this.document.location.origin,
      guid,
    )
  }

  private constructorFor(lease: () => Promise<AuthKeyLease | null>) {
    return noiseWebSocketConstructor(async () => {
      const current = await lease()
      if (!current) throw new Error('No active device identity')
      return this.prepare(current)
    }, this.context)
  }

  private async prepare(current: AuthKeyLease) {
    current.signal.throwIfAborted()
    let identity = this.identities.get(current.signal)
    if (!identity) {
      identity = prepareNoiseIdentity(current.key).catch(error => {
        this.identities.delete(current.signal)
        throw error
      })
      this.identities.set(current.signal, identity)
      current.signal.addEventListener(
        'abort',
        () => this.identities.delete(current.signal),
        { once: true },
      )
    }
    const prepared = await identity
    current.signal.throwIfAborted()
    return { identity: prepared, signal: current.signal }
  }
}
