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
} as const
