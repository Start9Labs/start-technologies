export const NOISE_PROFILE = {
  identity: {
    protocol: 'Noise_XX_25519_ChaChaPoly_SHA256',
    signaturePrefix: [
      110, 111, 105, 115, 101, 45, 108, 105, 98, 112, 50, 112, 45, 115, 116, 97,
      116, 105, 99, 45, 107, 101, 121, 58,
    ],
  },
  transport: {
    recordHeaderBytes: 2,
    messageHeaderBytes: 4,
    messageKindBytes: 1,
    maxRecordBytes: 65535,
    maxPlaintextBytes: 65519,
    maxHandshakeRecordBytes: 1024,
    handshakeTimeoutMs: 30000,
    writeTimeoutMs: 15000,
    maxIncomingBytes: 67108864,
    maxWireBytes: 67127319,
    maxWireChunkBytes: 65537,
    closeStatusBytes: 2,
    normalCloseCode: 1000,
    noStatusCloseCode: 1005,
    abnormalCloseCode: 1006,
    maxClosePayloadBytes: 125,
    maxCloseReasonBytes: 123,
    closeCodes: [
      [1000, 1003],
      [1007, 1013],
      [3000, 4999],
    ],
    kinds: {
      text: 0,
      binary: 1,
      close: 2,
    },
  },
  negotiation: {
    noise: 1,
    context: '',
  },
  prologuePrefix: 'Start-RPC-Noise v1\u0000',
  rpcPath: '/ws/rpc',
}
