import { defaultLogger } from '@libp2p/logger'
import { AbstractMessageStream, repeatingTask } from '@libp2p/utils'
import { NOISE_PROFILE } from '@start9labs/start-core/osBindings/noise-profile'
import type { Uint8ArrayList } from 'uint8arraylist'
import { withArrayBuffer } from 'uint8arrays/with-array-buffer'

const profile = NOISE_PROFILE.transport

export class NoiseWebSocketStream extends AbstractMessageStream {
  private readonly firstHeader = new Uint8Array(profile.recordHeaderBytes)
  private headerBytes = 0
  private readonly drain = repeatingTask(() => {
    if (this.socket.bufferedAmount === 0) {
      this.drain.stop()
      this.onMuxerDrain()
    }
  }, 10)

  constructor(private readonly socket: WebSocket) {
    super({
      direction: 'outbound',
      log: defaultLogger().forComponent('start9:noise:websocket'),
      maxMessageSize: profile.maxWireChunkBytes,
      maxReadBufferLength: profile.maxWireBytes,
      maxWriteBufferLength: profile.maxWireBytes,
    })
    socket.binaryType = 'arraybuffer'
    socket.addEventListener('message', event => {
      try {
        if (
          !(event.data instanceof ArrayBuffer) ||
          event.data.byteLength > profile.maxWireChunkBytes
        )
          throw new Error('Invalid encrypted WebSocket frame')
        const bytes = new Uint8Array(event.data)
        if (this.headerBytes < this.firstHeader.length) {
          const part = bytes.subarray(
            0,
            this.firstHeader.length - this.headerBytes,
          )
          this.firstHeader.set(part, this.headerBytes)
          this.headerBytes += part.length
          if (
            this.headerBytes === this.firstHeader.length &&
            new DataView(this.firstHeader.buffer).getUint16(0) >
              profile.maxHandshakeRecordBytes
          )
            throw new Error('Oversized Noise handshake record')
        }
        this.onData(bytes)
      } catch (error) {
        this.abort(error instanceof Error ? error : new Error(String(error)))
      }
    })
    socket.addEventListener('error', () => {
      this.onTransportClosed(new Error('WebSocket transport failed'))
    })
    socket.addEventListener(
      'close',
      () => {
        this.drain.stop()
        this.onTransportClosed()
      },
      { once: true },
    )
  }

  sendData(data: Uint8ArrayList) {
    this.socket.send(withArrayBuffer(data.subarray()))
    const canSendMore = this.socket.bufferedAmount < profile.maxWireChunkBytes
    if (!canSendMore) this.drain.start()
    return { sentBytes: data.byteLength, canSendMore }
  }

  sendReset() {
    this.drain.stop()
    this.socket.close()
  }

  sendPause() {}
  sendResume() {}

  async close() {
    this.drain.stop()
    this.socket.close()
  }
}
