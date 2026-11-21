export const SIGNATURE_PROFILE = {
  commitmentTag: [
    83, 116, 97, 114, 116, 45, 67, 111, 109, 109, 105, 116, 109, 101, 110, 116,
    45, 83, 105, 103, 32, 118, 49, 0,
  ],
  requestType: 3,
  requestName: 'request',
  ed25519: {
    keyPrefix: [48, 42, 48, 5, 6, 3, 43, 101, 112, 3, 33, 0],
    keyBytes: 32,
    signaturePrefix: [48, 73, 48, 5, 6, 3, 43, 101, 112, 4, 64],
  },
  ed25519ph: {
    signaturePrefix: [
      48, 89, 48, 21, 6, 19, 105, 174, 245, 245, 177, 231, 137, 138, 133, 201,
      156, 154, 227, 187, 133, 251, 242, 137, 91, 4, 64,
    ],
  },
  webauthn: {
    algorithm: [
      48, 22, 6, 20, 105, 130, 176, 132, 192, 167, 187, 205, 162, 186, 255, 169,
      236, 216, 239, 250, 128, 138, 191, 17,
    ],
  },
  p256: {
    keyPrefix: [
      48, 89, 48, 19, 6, 7, 42, 134, 72, 206, 61, 2, 1, 6, 8, 42, 134, 72, 206,
      61, 3, 1, 7, 3, 66, 0,
    ],
    keyBytes: 65,
    compressedKeyPrefix: [
      48, 57, 48, 19, 6, 7, 42, 134, 72, 206, 61, 2, 1, 6, 8, 42, 134, 72, 206,
      61, 3, 1, 7, 3, 34, 0,
    ],
    compressedKeyBytes: 33,
  },
  signatureBytes: 64,
  fingerprint: {
    algorithm: 'SHA-256',
    prefix: 'SHA256:',
  },
  http: {
    tag: [
      83, 116, 97, 114, 116, 45, 65, 117, 116, 104, 45, 83, 105, 103, 32, 118,
      50, 0,
    ],
    header: 'X-Start-Auth-Sig',
  },
  registry: {
    header: 'x-start-webauthn-signatures',
    value: 'true',
  },
} as const
