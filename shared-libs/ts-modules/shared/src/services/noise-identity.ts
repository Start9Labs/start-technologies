import { publicKeyFromRaw } from '@libp2p/crypto/keys'
import type {
  Ed25519PrivateKey,
  Ed25519PublicKey,
  MessageStream,
} from '@libp2p/interface'
import { defaultLogger } from '@libp2p/logger'
import { noise, pureJsCrypto } from '@libp2p/noise'
import type { NoiseComponents } from '@libp2p/noise'
import { auth } from '@start9labs/start-core'
import { NOISE_PROFILE } from '@start9labs/start-core/osBindings/noise-profile'
import { equals } from 'uint8arrays/equals'

export async function prepareNoiseIdentity(key: auth.AuthKey) {
  const dh = pureJsCrypto.generateX25519KeyPair()
  const message = new Uint8Array([
    ...NOISE_PROFILE.identity.signaturePrefix,
    ...dh.publicKey,
  ])
  const signature = new Uint8Array(
    await crypto.subtle.sign('Ed25519', key.privateKey, message),
  )
  const publicKey = publicKeyFromRaw(
    auth.parseEd25519PublicKey(key.pubkeyPem),
  ) as Ed25519PublicKey
  const privateKey: Ed25519PrivateKey = {
    type: 'Ed25519',
    publicKey,
    get raw(): Uint8Array {
      throw new Error('The identity key is not extractable')
    },
    equals: other => other === privateKey,
    sign(data, options) {
      options?.signal?.throwIfAborted()
      if (!equals(data.subarray(), message))
        throw new Error('Unexpected Noise identity signing message')
      return signature
    },
  }
  const components = { privateKey, logger: defaultLogger() }

  return (context: string, stream: MessageStream, signal: AbortSignal) =>
    noise({
      staticNoiseKey: dh.privateKey,
      prologueBytes: new TextEncoder().encode(
        NOISE_PROFILE.prologuePrefix + context,
      ),
    })(components as NoiseComponents).secureOutbound(stream, {
      signal,
      // Skipping muxer negotiation leaves peerId and upgrader unused.
      skipStreamMuxerNegotiation: true,
    })
}

export type PreparedNoiseIdentity = Awaited<
  ReturnType<typeof prepareNoiseIdentity>
>
