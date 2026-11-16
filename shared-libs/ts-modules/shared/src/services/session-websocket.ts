import { auth } from '@start9labs/start-core'
import type { Target } from '@start9labs/start-core/osBindings/session'
import { SESSION_PROFILE } from '@start9labs/start-core/osBindings/session-profile'
import { pEvent } from 'p-event'

import type { SessionCipher } from './session-identity'
import { SessionWebSocketStream } from './session-stream'

const profile = SESSION_PROFILE.transport
const encoder = new TextEncoder()
const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true })

export interface SessionSocketIdentity {
  readonly signer: auth.RequestSigner
  readonly signal: AbortSignal
}

export class SessionOpenEvent extends Event {
  constructor(readonly peer: string) {
    super('open')
  }
}

function validateCloseCode(code: number) {
  if (
    !profile.closeCodes.some(([start, end]) => code >= start! && code <= end!)
  )
    throw new DOMException('Invalid close code', 'InvalidAccessError')
}

function closePayload(code?: number, reason = ''): Uint8Array {
  const text = encoder.encode(reason)
  if (text.length > profile.maxCloseReasonBytes)
    throw new DOMException('Close reason is too long', 'SyntaxError')
  if (code === undefined && !reason) return new Uint8Array()
  code ??= profile.normalCloseCode
  validateCloseCode(code)
  const payload = new Uint8Array(profile.closeStatusBytes + text.length)
  new DataView(payload.buffer).setUint16(0, code)
  payload.set(text, profile.closeStatusBytes)
  return payload
}

function validateClosePayload(payload: Uint8Array): void {
  if (payload.length === 0) return
  if (
    payload.length < profile.closeStatusBytes ||
    payload.length > profile.maxClosePayloadBytes
  )
    throw new Error('Invalid encrypted close result')
  const code = new DataView(
    payload.buffer,
    payload.byteOffset,
    payload.byteLength,
  ).getUint16(0)
  decoder.decode(payload.subarray(profile.closeStatusBytes))
  validateCloseCode(code)
}

export function sessionWebSocketConstructor(
  prepare: () => Promise<SessionSocketIdentity>,
  context: string,
  target: Target,
  Socket: typeof WebSocket = WebSocket,
): typeof WebSocket {
  return class SessionWebSocket extends EventTarget implements WebSocket {
    static readonly CONNECTING = 0
    static readonly OPEN = 1
    static readonly CLOSING = 2
    static readonly CLOSED = 3
    readonly CONNECTING = 0
    readonly OPEN = 1
    readonly CLOSING = 2
    readonly CLOSED = 3
    readonly url: string
    binaryType: BinaryType = 'blob'
    readyState: WebSocket['readyState'] = this.CONNECTING
    onopen: WebSocket['onopen'] = null
    onmessage: WebSocket['onmessage'] = null
    onerror: WebSocket['onerror'] = null
    onclose: WebSocket['onclose'] = null
    private readonly lifetime = new AbortController()
    private socket?: WebSocket
    private raw?: SessionWebSocketStream
    private cipher?: SessionCipher
    private plaintext = new Uint8Array()
    private offset = 0
    private writer = Promise.resolve()
    private queuedBytes = 0

    constructor(url: string | URL, protocols?: string | string[]) {
      super()
      const endpoint = new URL(url)
      this.url = endpoint.href
      for (const type of ['open', 'message', 'error', 'close'] as const)
        this.addEventListener(type, event => {
          const handler = this[`on${type}`]
          if (handler) Reflect.apply(handler, this, [event])
        })
      void this.connect(protocols).catch(error => this.fail(error))
    }

    get extensions() {
      return this.socket?.extensions ?? ''
    }

    get protocol() {
      return this.socket?.protocol ?? ''
    }

    get bufferedAmount() {
      return this.queuedBytes + (this.socket?.bufferedAmount ?? 0)
    }

    private async connect(protocols?: string | string[]) {
      const identity = await prepare()
      if (this.readyState !== this.CONNECTING) return
      identity.signal.throwIfAborted()
      identity.signal.addEventListener(
        'abort',
        () => this.fail(identity.signal.reason),
        { once: true, signal: this.lifetime.signal },
      )
      const { prepareSession } = await import('./session-identity')
      const prepared = await prepareSession(identity.signer, context, target)
      if (this.readyState !== this.CONNECTING) return
      identity.signal.throwIfAborted()
      const signal = AbortSignal.any([
        identity.signal,
        this.lifetime.signal,
        AbortSignal.timeout(profile.handshakeTimeoutMs),
      ])
      const socket = (this.socket = new Socket(this.url, protocols))
      const raw = (this.raw = new SessionWebSocketStream(socket))
      const abort = () => raw.abort(new Error('Encrypted WebSocket aborted'))
      signal.addEventListener('abort', abort, { once: true })
      let peer: string
      try {
        await pEvent(socket, 'open', {
          signal,
          rejectionEvents: ['error', 'close'],
        })
        await raw.writeRecord(prepared.message, signal)
        const response = await raw.readRecord(
          profile.maxHandshakeRecordBytes,
          signal,
        )
        const secured = await prepared.finish(response)
        signal.throwIfAborted()
        peer = secured.peer
        this.cipher = secured.cipher
      } finally {
        signal.removeEventListener('abort', abort)
      }
      this.lifetime.signal.addEventListener('abort', abort, { once: true })
      this.readyState = this.OPEN
      this.dispatchEvent(new SessionOpenEvent(peer))
      while (this.readyState < this.CLOSED) {
        const header = await this.readPlain(profile.messageHeaderBytes)
        const length = new DataView(header.buffer).getUint32(0)
        if (
          length < profile.messageKindBytes ||
          length > profile.maxIncomingBytes + profile.messageKindBytes
        )
          throw new Error('Invalid encrypted WebSocket message length')
        const bytes = await this.readPlain(length)
        this.lifetime.signal.throwIfAborted()
        const kind = bytes[0]
        const payload = bytes.subarray(profile.messageKindBytes)
        if (kind === profile.kinds.close) {
          validateClosePayload(payload)
          if (
            this.offset !== this.plaintext.length ||
            raw.readBufferLength !== 0
          )
            throw new Error('Data follows encrypted close result')
          this.fail(new Error('Encrypted WebSocket closed'))
          return
        }
        if (payload.length > profile.maxIncomingBytes)
          throw new Error('Oversized encrypted WebSocket message')
        let data: string | ArrayBuffer | Blob
        if (kind === profile.kinds.text) data = decoder.decode(payload)
        else if (kind === profile.kinds.binary)
          data =
            this.binaryType === 'blob'
              ? new Blob([new Uint8Array(payload)])
              : new Uint8Array(payload).buffer
        else throw new Error('Invalid encrypted WebSocket message kind')
        this.dispatchEvent(new MessageEvent('message', { data }))
      }
    }

    send(data: string | ArrayBufferLike | Blob | ArrayBufferView) {
      if (this.readyState === this.CONNECTING)
        throw new DOMException('WebSocket is connecting', 'InvalidStateError')
      if (this.readyState !== this.OPEN) return
      const kind =
        typeof data === 'string' ? profile.kinds.text : profile.kinds.binary
      const bytes =
        typeof data === 'string'
          ? encoder.encode(data)
          : data instanceof Blob
            ? data
            : ArrayBuffer.isView(data)
              ? new Uint8Array(
                  data.buffer,
                  data.byteOffset,
                  data.byteLength,
                ).slice()
              : new Uint8Array(data).slice()
      this.queue(kind, bytes)
    }

    close(code?: number, reason = '') {
      const payload = closePayload(code, reason)
      if (this.readyState >= this.CLOSING) return
      if (this.readyState === this.CONNECTING) {
        this.finish(
          new CloseEvent('close', { code: profile.abnormalCloseCode }),
        )
        return
      }
      this.readyState = this.CLOSING
      this.queue(profile.kinds.close, payload)
      void this.writer.then(() => this.socket?.close())
    }

    private queue(kind: number, payload: Uint8Array | Blob) {
      const length = payload instanceof Blob ? payload.size : payload.byteLength
      if (
        kind !== profile.kinds.close &&
        this.queuedBytes + length > profile.maxIncomingBytes
      ) {
        this.fail(new Error('Encrypted WebSocket write buffer is full'))
        return
      }
      this.queuedBytes += length
      this.writer = this.writer
        .then(async () => {
          const signal = AbortSignal.any([
            this.lifetime.signal,
            AbortSignal.timeout(profile.writeTimeoutMs),
          ])
          signal.throwIfAborted()
          const bytes =
            payload instanceof Blob
              ? new Uint8Array(await payload.arrayBuffer())
              : payload
          const length = profile.messageKindBytes + bytes.length
          const message = new Uint8Array(profile.messageHeaderBytes + length)
          new DataView(message.buffer).setUint32(0, length)
          message[profile.messageHeaderBytes] = kind
          message.set(
            bytes,
            profile.messageHeaderBytes + profile.messageKindBytes,
          )
          for (
            let offset = 0;
            offset < message.length;
            offset += profile.maxPlaintextBytes
          ) {
            signal.throwIfAborted()
            const encrypted = await this.cipher!.write(
              message.subarray(offset, offset + profile.maxPlaintextBytes),
            )
            await this.raw!.writeRecord(encrypted, signal)
          }
        })
        .catch(error => this.fail(error))
        .finally(() => {
          this.queuedBytes -= length
        })
    }

    private async readPlain(length: number): Promise<Uint8Array<ArrayBuffer>> {
      const result = new Uint8Array(length)
      let written = 0
      while (written < length) {
        if (this.offset === this.plaintext.length) {
          const record = await this.raw!.readRecord(
            profile.maxRecordBytes,
            this.lifetime.signal,
          )
          const plaintext = await this.cipher!.read(record)
          this.lifetime.signal.throwIfAborted()
          this.plaintext = plaintext
          if (this.plaintext.length === 0)
            throw new Error('Empty encrypted record')
          this.offset = 0
        }
        const count = Math.min(
          length - written,
          this.plaintext.length - this.offset,
        )
        result.set(
          this.plaintext.subarray(this.offset, this.offset + count),
          written,
        )
        this.offset += count
        written += count
      }
      return result
    }

    private fail(error: unknown) {
      if (this.readyState === this.CLOSED) return
      if (this.readyState !== this.CLOSING) {
        this.readyState = this.CLOSING
        this.dispatchEvent(new ErrorEvent('error', { error }))
      }
      this.finish(new CloseEvent('close', { code: profile.abnormalCloseCode }))
    }

    private finish(event: CloseEvent) {
      if (this.readyState === this.CLOSED) return
      this.readyState = this.CLOSED
      this.lifetime.abort()
      this.plaintext = new Uint8Array()
      this.offset = 0
      this.dispatchEvent(event)
      if (this.raw) this.raw.abort(this.lifetime.signal.reason)
      else this.socket?.close()
    }
  }
}
