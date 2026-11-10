import { auth, z } from '@start9labs/start-core'
import { SESSION_PROFILE } from '@start9labs/start-core/osBindings/session-profile'
import type { Target, Message } from '@start9labs/start-core/osBindings/session'
import {
  Envelope,
  Message as MessageSchema,
} from '@start9labs/start-core/osBindings/session.schemas'

export { verifyingKeyFingerprint } from '@start9labs/start-core/auth/signature'

const profile = SESSION_PROFILE.identity
const maximum = SESSION_PROFILE.transport.maxHandshakeRecordBytes
const encoder = new TextEncoder()
const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true })

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

function parseHandshake<T>(schema: z.ZodType<T>, bytes: Uint8Array): T {
  const raw: unknown = JSON.parse(decoder.decode(bytes))
  const parsed = schema.parse(raw)
  // Generated RPC schemas strip unknown fields; admission rejects them.
  sameFields(raw, parsed)
  return parsed
}

function decodeBase64(value: string, length?: number): Uint8Array<ArrayBuffer> {
  if (!/^[A-Za-z0-9+/]*={0,2}$/.test(value))
    throw new Error('Invalid handshake Base64')
  const normalized = value.padEnd(Math.ceil(value.length / 4) * 4, '=')
  const bytes = new Uint8Array(auth.base64ToBytes(normalized))
  if (
    auth.bytesToBase64(bytes) !== normalized ||
    (length !== undefined && bytes.length !== length)
  )
    throw new Error('Invalid handshake Base64')
  return bytes
}

async function digest(bytes: Uint8Array): Promise<Uint8Array<ArrayBuffer>> {
  return new Uint8Array(
    await crypto.subtle.digest(profile.hash, new Uint8Array(bytes)),
  )
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
    private readonly key: CryptoKey,
    direction: number,
  ) {
    this.aad = new Uint8Array([...profile.aadPrefix, direction])
  }

  private parameters(): AesGcmParams {
    if (this.counter >= profile.maxRecordsPerKey)
      throw new Error('Session record key exhausted')
    const iv = new Uint8Array(profile.nonceBytes)
    new DataView(iv.buffer).setBigUint64(iv.length - 8, BigInt(this.counter++))
    return {
      name: profile.cipher,
      iv,
      additionalData: this.aad,
      tagLength: profile.tagBytes * 8,
    }
  }

  async write(bytes: Uint8Array): Promise<Uint8Array<ArrayBuffer>> {
    return new Uint8Array(
      await crypto.subtle.encrypt(
        this.parameters(),
        this.key,
        new Uint8Array(bytes),
      ),
    )
  }

  async read(bytes: Uint8Array): Promise<Uint8Array<ArrayBuffer>> {
    return new Uint8Array(
      await crypto.subtle.decrypt(
        this.parameters(),
        this.key,
        new Uint8Array(bytes),
      ),
    )
  }
}

/** The socket owner serializes records and terminates the channel on authentication failure. */
export interface SessionCipher {
  write(bytes: Uint8Array): Promise<Uint8Array<ArrayBuffer>>
  read(bytes: Uint8Array): Promise<Uint8Array<ArrayBuffer>>
}

/** Prepares a single connection before its network deadline, without exporting private keys. */
export async function prepareSession(
  signer: auth.RequestSigner,
  context: string,
  target: Target,
) {
  const dh = (await crypto.subtle.generateKey(profile.dh, false, [
    'deriveBits',
  ])) as CryptoKeyPair
  const ephemeral = new Uint8Array(
    await crypto.subtle.exportKey('raw', dh.publicKey),
  )
  const body: Message = {
    protocol: profile.protocol,
    version: profile.version,
    hello: { role: 'client', ephemeral: auth.bytesToBase64(ephemeral), target },
  }
  const payload = encoder.encode(JSON.stringify(body))
  const proof = await signer.sign(context, payload)
  const wire = encoder.encode(
    JSON.stringify({ payload: auth.bytesToBase64(payload), proof }),
  )
  if (wire.length > maximum) throw new Error('Oversized signed handshake')
  let secret: CryptoKey | undefined = dh.privateKey
  return {
    message: wire.slice(),
    async finish(
      response: Uint8Array,
    ): Promise<{ peer: string; cipher: SessionCipher }> {
      const key = secret
      secret = undefined
      if (!key) throw new Error('Session initiation was already consumed')
      if (response.length > maximum)
        throw new Error('Oversized signed handshake')
      const envelope = parseHandshake(Envelope, response)
      const payload = decodeBase64(envelope.payload)
      const peer = auth.verifyRequestProof(envelope.proof, context, payload)
      const message = parseHandshake(MessageSchema, payload)
      if (
        message.protocol !== profile.protocol ||
        message.version !== profile.version ||
        message.hello.role !== 'server'
      )
        throw new Error('Invalid server handshake')
      const hello = message.hello
      const clientHash = await digest(wire)
      if (
        !decodeBase64(hello.clientHello, clientHash.length).every(
          (byte, i) => byte === clientHash[i],
        )
      )
        throw new Error('Server handshake does not bind this initiation')
      const remote = await crypto.subtle.importKey(
        'raw',
        decodeBase64(hello.ephemeral, ephemeral.length),
        profile.dh,
        false,
        [],
      )
      const shared = new Uint8Array(
        await crypto.subtle.deriveBits(
          { name: profile.dh, public: remote },
          key,
          profile.keyBytes * 8,
        ),
      )
      if (shared.every(byte => byte === 0))
        throw new Error('Noncontributory DH key')
      const material = await crypto.subtle.importKey(
        'raw',
        shared,
        profile.kdf,
        false,
        ['deriveKey'],
      )
      shared.fill(0)
      const salt = await digest(transcript(wire, response))
      const derive = (info: readonly number[]) =>
        crypto.subtle.deriveKey(
          {
            name: profile.kdf,
            hash: profile.hash,
            salt,
            info: new Uint8Array(info),
          },
          material,
          { name: profile.cipher, length: profile.keyBytes * 8 },
          false,
          ['encrypt', 'decrypt'],
        )
      const [send, receive] = await Promise.all([
        derive(profile.clientKeyInfo),
        derive(profile.serverKeyInfo),
      ])
      const outgoing = new RecordKey(send, profile.directions.client)
      const incoming = new RecordKey(receive, profile.directions.server)
      return {
        peer,
        cipher: {
          write: bytes => outgoing.write(bytes),
          read: bytes => incoming.read(bytes),
        },
      }
    },
  }
}
