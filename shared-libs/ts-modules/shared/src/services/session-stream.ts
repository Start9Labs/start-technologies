import { SESSION_PROFILE } from '@start9labs/start-core/osBindings/session-profile'

const profile = SESSION_PROFILE.transport

export class SessionWebSocketStream {
  private readonly reader: ReadableStreamDefaultReader<Uint8Array<ArrayBuffer>>
  private controller!: ReadableStreamDefaultController<Uint8Array<ArrayBuffer>>
  private ended = false
  private chunk = new Uint8Array()
  private offset = 0
  private buffered = 0

  constructor(private readonly socket: WebSocket) {
    const input = new ReadableStream<Uint8Array<ArrayBuffer>>(
      {
        start: controller => {
          this.controller = controller
        },
      },
      { highWaterMark: profile.maxWireBytes, size: bytes => bytes.length },
    )
    this.reader = input.getReader()
    void this.reader.closed.catch(() => {})
    socket.binaryType = 'arraybuffer'
    socket.addEventListener('message', event => {
      if (this.ended) return
      if (
        !(event.data instanceof ArrayBuffer) ||
        event.data.byteLength > profile.maxWireChunkBytes ||
        this.buffered + event.data.byteLength > profile.maxWireBytes
      ) {
        this.abort(new Error('Invalid or oversized encrypted WebSocket frame'))
        return
      }
      this.buffered += event.data.byteLength
      this.controller.enqueue(new Uint8Array(event.data))
    })
    socket.addEventListener('error', () =>
      this.abort(new Error('WebSocket transport failed')),
    )
    socket.addEventListener(
      'close',
      () => {
        if (this.ended) return
        this.ended = true
        this.controller.close()
      },
      { once: true },
    )
  }

  get readBufferLength() {
    return this.buffered
  }

  abort(reason: unknown) {
    if (!this.ended) {
      this.ended = true
      this.controller.error(reason)
    }
    this.chunk = new Uint8Array()
    this.offset = 0
    this.buffered = 0
    void this.reader.cancel(reason).catch(() => {})
    this.socket.close()
  }

  async read(
    length: number,
    signal: AbortSignal,
  ): Promise<Uint8Array<ArrayBuffer>> {
    const abort = () => this.abort(signal.reason)
    signal.throwIfAborted()
    signal.addEventListener('abort', abort, { once: true })
    try {
      const result = new Uint8Array(length)
      let written = 0
      while (written < length) {
        if (this.offset === this.chunk.length) {
          const next = await this.reader.read()
          signal.throwIfAborted()
          if (next.done || (this.ended && this.buffered === 0))
            throw new Error(
              'Encrypted WebSocket ended without a verified close',
            )
          this.chunk = next.value
          this.offset = 0
        }
        const count = Math.min(
          length - written,
          this.chunk.length - this.offset,
        )
        result.set(
          this.chunk.subarray(this.offset, this.offset + count),
          written,
        )
        this.offset += count
        written += count
        this.buffered -= count
      }
      return result
    } finally {
      signal.removeEventListener('abort', abort)
    }
  }

  async readRecord(
    maximum: number,
    signal: AbortSignal,
  ): Promise<Uint8Array<ArrayBuffer>> {
    const header = await this.read(profile.recordHeaderBytes, signal)
    const length = new DataView(header.buffer).getUint16(0)
    if (length > maximum) throw new Error('Oversized encrypted session record')
    return this.read(length, signal)
  }

  async writeRecord(bytes: Uint8Array, signal: AbortSignal): Promise<void> {
    signal.throwIfAborted()
    while (this.socket.bufferedAmount >= profile.maxWireChunkBytes) {
      await new Promise(resolve => setTimeout(resolve, 10))
      signal.throwIfAborted()
    }
    if (this.ended) throw new Error('Encrypted WebSocket is closed')
    const frame = new Uint8Array(profile.recordHeaderBytes + bytes.length)
    new DataView(frame.buffer).setUint16(0, bytes.length)
    frame.set(bytes, profile.recordHeaderBytes)
    this.socket.send(frame)
  }
}
