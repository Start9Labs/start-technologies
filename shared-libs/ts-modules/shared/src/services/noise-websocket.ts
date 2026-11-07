import { lpStream } from '@libp2p/utils'
import { NOISE_PROFILE } from '@start9labs/start-core/osBindings/noise-profile'
import { pEvent } from 'p-event'

import type { PreparedNoiseIdentity } from './noise-identity'
import { NoiseWebSocketStream } from './noise-stream'

const profile = NOISE_PROFILE.transport
const encoder = new TextEncoder()
const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true })

export interface NoiseSocketIdentity {
  identity: PreparedNoiseIdentity
  signal: AbortSignal
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

function closeEvent(payload: Uint8Array): CloseEvent {
  if (payload.length === 0)
    return new CloseEvent('close', {
      code: profile.noStatusCloseCode,
      wasClean: true,
    })
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
  const reason = decoder.decode(payload.subarray(profile.closeStatusBytes))
  validateCloseCode(code)
  return new CloseEvent('close', { code, reason, wasClean: true })
}

export function noiseWebSocketConstructor(
  prepare: () => Promise<NoiseSocketIdentity>,
  context: string,
  Socket: typeof WebSocket = WebSocket,
): typeof WebSocket {
  return class NoiseWebSocket extends EventTarget implements WebSocket {
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
    private messages?: ReturnType<typeof lpStream>
    private writer = Promise.resolve()
    private queuedBytes = 0

    constructor(url: string | URL, protocols?: string | string[]) {
      super()
      const endpoint = new URL(url)
      const negotiation = { ...NOISE_PROFILE.negotiation }
      negotiation.context = context
      for (const [name, value] of Object.entries(negotiation))
        endpoint.searchParams.set(name, String(value))
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
      const prepared = await prepare()
      if (this.readyState !== this.CONNECTING) return
      prepared.signal.throwIfAborted()
      prepared.signal.addEventListener(
        'abort',
        () => this.fail(prepared.signal.reason),
        { once: true, signal: this.lifetime.signal },
      )
      const signal = AbortSignal.any([
        prepared.signal,
        this.lifetime.signal,
        AbortSignal.timeout(profile.handshakeTimeoutMs),
      ])
      const socket = (this.socket = new Socket(this.url, protocols))
      const raw = new NoiseWebSocketStream(socket)
      const abort = () => raw.abort(new Error('Encrypted WebSocket aborted'))
      signal.addEventListener('abort', abort, { once: true })
      try {
        await pEvent(socket, 'open', {
          signal,
          rejectionEvents: ['error', 'close'],
        })
        const secured = await prepared.identity(context, raw, signal)
        signal.throwIfAborted()
        if (secured.remotePeer.publicKey?.type !== 'Ed25519')
          throw new Error('Unsupported server identity')
        this.messages = lpStream(secured.connection, {
          maxDataLength: profile.maxIncomingBytes + profile.messageKindBytes,
          maxBufferSize:
            profile.maxIncomingBytes +
            profile.messageKindBytes +
            profile.messageHeaderBytes +
            profile.maxPlaintextBytes,
          maxLengthLength: profile.messageHeaderBytes,
          lengthEncoder(length) {
            const bytes = new Uint8Array(profile.messageHeaderBytes)
            new DataView(bytes.buffer).setUint32(0, length)
            return bytes
          },
          lengthDecoder: bytes => bytes.getUint32(0),
        })
      } finally {
        signal.removeEventListener('abort', abort)
      }
      this.lifetime.signal.addEventListener('abort', abort, { once: true })
      this.readyState = this.OPEN
      this.dispatchEvent(new Event('open'))
      while (this.readyState < this.CLOSED) {
        const bytes = (
          await this.messages!.read({ signal: this.lifetime.signal })
        ).subarray()
        const kind = bytes[0]
        const payload = bytes.subarray(profile.messageKindBytes)
        if (kind === profile.kinds.close) {
          const event = closeEvent(payload)
          if (this.messages!.unwrap().readBufferLength !== 0)
            throw new Error('Data follows encrypted close result')
          this.finish(event)
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
          const message = new Uint8Array(
            profile.messageKindBytes + bytes.length,
          )
          message[0] = kind
          message.set(bytes, profile.messageKindBytes)
          await this.messages!.write(message, { signal })
        })
        .catch(error => this.fail(error))
        .finally(() => {
          this.queuedBytes -= length
        })
    }

    private fail(error: unknown) {
      if (this.readyState === this.CLOSED) return
      this.readyState = this.CLOSING
      this.dispatchEvent(new ErrorEvent('error', { error }))
      this.finish(new CloseEvent('close', { code: profile.abnormalCloseCode }))
    }

    private finish(event: CloseEvent) {
      if (this.readyState === this.CLOSED) return
      this.readyState = this.CLOSED
      this.lifetime.abort()
      this.dispatchEvent(event)
      this.socket?.close()
    }
  }
}
