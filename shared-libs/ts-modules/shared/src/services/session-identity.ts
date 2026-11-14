import { gcm } from '@noble/ciphers/aes.js'
import { x25519 } from '@noble/curves/ed25519'
import { hkdf } from '@noble/hashes/hkdf'
import { sha256 } from '@noble/hashes/sha256'
import { auth, z } from '@start9labs/start-core'
import { decodeFirst, encode } from '@start9labs/start-core/auth/cbor'
import { SESSION_PROFILE } from '@start9labs/start-core/osBindings/session-profile'
import type { Target, Message } from '@start9labs/start-core/osBindings/session'
import { Message as MessageSchema } from '@start9labs/start-core/osBindings/session.schemas'

export { verifyingKeyFingerprint } from '@start9labs/start-core/auth/signature'

const profile = SESSION_PROFILE.identity
const maximum = SESSION_PROFILE.transport.maxHandshakeRecordBytes

function sameFields(original: unknown, parsed: unknown): void {
  if (original === null || typeof original !== 'object') return
  const raw = original as Record<string, unknown>
  const shaped = parsed as Record<string, unknown>
  const keys = Object.keys(raw)
  if (
    keys.length !== Object.keys(shaped).length ||
    keys.some(key => !Object.hasOwn(shaped, key))
  )
    throw new Error('Unknown handshake field')
  for (const key of keys) sameFields(raw[key], shaped[key])
}

function parseHandshake<T>(schema: z.ZodType<T>, raw: unknown): T {
  const parsed = schema.parse(raw)
  // Generated RPC schemas strip unknown fields; admission rejects them.
  sameFields(raw, parsed)
  return parsed
}

function digest(bytes: Uint8Array): Uint8Array<ArrayBuffer> {
  return new Uint8Array(sha256(bytes))
}

function transcript(
  client: Uint8Array,
  server: Uint8Array,
): Uint8Array<ArrayBuffer> {
  const bytes = new Uint8Array(8 + client.length + server.length)
  const view = new DataView(bytes.buffer)
  view.setUint32(0, client.length)
  bytes.set(client, 4)
  view.setUint32(4 + client.length, server.length)
  bytes.set(server, 8 + client.length)
  return bytes
}

class RecordKey {
  private counter = 0
  private readonly aad: Uint8Array<ArrayBuffer>

  constructor(
    private readonly key: Uint8Array,
    direction: number,
  ) {
    this.aad = new Uint8Array([...profile.aadPrefix, direction])
  }

  private nonce(): Uint8Array<ArrayBuffer> {
    if (this.counter >= profile.maxRecordsPerKey)
      throw new Error('Session record key exhausted')
    const iv = new Uint8Array(profile.nonceBytes)
    new DataView(iv.buffer).setBigUint64(iv.length - 8, BigInt(this.counter++))
    return iv
  }

  async write(bytes: Uint8Array): Promise<Uint8Array<ArrayBuffer>> {
    return new Uint8Array(gcm(this.key, this.nonce(), this.aad).encrypt(bytes))
  }

  async read(bytes: Uint8Array): Promise<Uint8Array<ArrayBuffer>> {
    return new Uint8Array(gcm(this.key, this.nonce(), this.aad).decrypt(bytes))
  }
}

/** The socket owner serializes records and terminates the channel on authentication failure. */
export interface SessionCipher {
  write(bytes: Uint8Array): Promise<Uint8Array<ArrayBuffer>>
  read(bytes: Uint8Array): Promise<Uint8Array<ArrayBuffer>>
}

/** Single-use initiation supports LAN HTTP without crypto.subtle; signing precedes network deadlines. */
export async function prepareSession(
  signer: auth.RequestSigner,
  context: string,
  target: Target,
) {
  const dh = x25519.utils.randomPrivateKey()
  const ephemeral = new Uint8Array(x25519.getPublicKey(dh))
  const body: Message = {
    protocol: profile.protocol,
    version: profile.version,
    hello: { role: 'client', ephemeral, target },
  }
  const payload = encode(body)
  const proof = await signer.sign(context, payload)
  const wire = new Uint8Array(payload.length + proof.length)
  wire.set(payload)
  wire.set(proof, payload.length)
  if (wire.length > maximum) throw new Error('Oversized signed handshake')
  let secret: Uint8Array | undefined = dh
  return {
    message: wire.slice(),
    async finish(
      response: Uint8Array,
    ): Promise<{ peer: string; cipher: SessionCipher }> {
      const key = secret
      secret = undefined
      if (!key) throw new Error('Session initiation was already consumed')
      try {
        if (response.length > maximum)
          throw new Error('Oversized signed handshake')
        const captured = await decodeFirst(response)
        const peer = await auth.verifyRequestProof(
          response.subarray(captured.length),
          context,
          response.subarray(0, captured.length),
        )
        const message = parseHandshake(MessageSchema, captured.value)
        if (
          message.protocol !== profile.protocol ||
          message.version !== profile.version ||
          message.hello.role !== 'server'
        )
          throw new Error('Invalid server handshake')
        const hello = message.hello
        const clientHash = await digest(wire)
        if (
          hello.clientHello.length !== clientHash.length ||
          !hello.clientHello.every((byte, i) => byte === clientHash[i])
        )
          throw new Error('Server handshake does not bind this initiation')
        const shared = x25519.getSharedSecret(
          key,
          new Uint8Array(hello.ephemeral),
        )
        const salt = digest(transcript(wire, response))
        const derive = (info: readonly number[]) =>
          hkdf(sha256, shared, salt, new Uint8Array(info), profile.keyBytes)
        const send = derive(profile.clientKeyInfo)
        const receive = derive(profile.serverKeyInfo)
        shared.fill(0)
        const outgoing = new RecordKey(send, profile.directions.client)
        const incoming = new RecordKey(receive, profile.directions.server)
        return {
          peer,
          cipher: {
            write: bytes => outgoing.write(bytes),
            read: bytes => incoming.read(bytes),
          },
        }
      } finally {
        key.fill(0)
      }
    },
  }
}
