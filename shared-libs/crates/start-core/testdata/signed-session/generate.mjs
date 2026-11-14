import assert from 'node:assert/strict'
import { createPrivateKey, createPublicKey, webcrypto } from 'node:crypto'
import { writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'

const require = createRequire(
  new URL('../../../../ts-modules/start-core/package.json', import.meta.url),
)
const { blake3 } = require('@noble/hashes/blake3')
const CBOR = require('cbor-web')
const encode = value =>
  CBOR.encodeOne(value, {
    canonical: true,
    genTypes: [
      Uint8Array,
      (encoder, bytes) => encoder.pushAny(bytes.slice().buffer),
    ],
  })
const { subtle } = webcrypto
const utf8 = value => new TextEncoder().encode(value)
const concat = (...parts) => Buffer.concat(parts.map(part => Buffer.from(part)))
const hex = value => Buffer.from(value, 'hex')
const b64 = value => Buffer.from(value).toString('base64')
const hash = async value =>
  new Uint8Array(await subtle.digest('SHA-256', value))
const context = 'server.test'

async function identity(seed) {
  const der = concat(
    hex('302e020100300506032b657004220420'),
    Buffer.alloc(32, seed),
  )
  const privateKey = await subtle.importKey('pkcs8', der, 'Ed25519', false, [
    'sign',
  ])
  const publicDer = createPublicKey(
    createPrivateKey({ key: der, format: 'der', type: 'pkcs8' }),
  ).export({ format: 'der', type: 'spki' })
  assert.equal(privateKey.extractable, false)
  return { privateKey, publicDer }
}

async function dh(privateHex) {
  const der = concat(hex('302e020100300506032b656e04220420'), hex(privateHex))
  const privateKey = await subtle.importKey('pkcs8', der, 'X25519', false, [
    'deriveBits',
  ])
  const publicDer = createPublicKey(
    createPrivateKey({ key: der, format: 'der', type: 'pkcs8' }),
  ).export({ format: 'der', type: 'spki' })
  return { privateKey, public: publicDer.subarray(-32) }
}

async function signed(hello, key, nonce) {
  const payload = encode({ protocol: 'Start-RPC-Session', version: 1, hello })
  const commitment = new Uint8Array(56)
  const view = new DataView(commitment.buffer)
  const timestamp = 1792644825n
  view.setBigInt64(0, timestamp, false)
  view.setBigUint64(8, nonce, false)
  view.setBigUint64(16, BigInt(payload.length), false)
  commitment.set(blake3(payload), 24)
  const message = concat(
    utf8('Start-Commitment-Sig v1\0'),
    [utf8(context).length],
    utf8(context),
    [3],
    commitment,
  )
  const signature = await subtle.sign('Ed25519', key.privateKey, message)
  const proof = encode({
    commitment: {
      timestamp,
      nonce,
      size: BigInt(payload.length),
      blake3: commitment.subarray(24),
    },
    signer: new Uint8Array(key.publicDer),
    signature: new Uint8Array(concat(hex('3049300506032b65700440'), signature)),
  })
  return concat(payload, proof)
}

const clientPrivate =
  '77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a'
const serverPrivate =
  '5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb'
const clientDh = await dh(clientPrivate)
const serverDh = await dh(serverPrivate)
const clientIdentity = await identity(31)
const serverIdentity = await identity(32)
const client = await signed(
  { role: 'client', ephemeral: new Uint8Array(clientDh.public), target: 'rpc' },
  clientIdentity,
  0xffffffffffffffffn,
)
const server = await signed(
  {
    role: 'server',
    ephemeral: new Uint8Array(serverDh.public),
    clientHello: await hash(client),
  },
  serverIdentity,
  0xfffffffffffffffen,
)

const remote = await subtle.importKey(
  'raw',
  serverDh.public,
  'X25519',
  false,
  [],
)
const shared = new Uint8Array(
  await subtle.deriveBits(
    { name: 'X25519', public: remote },
    clientDh.privateKey,
    256,
  ),
)
assert.equal(
  Buffer.from(shared).toString('hex'),
  '4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742',
)
const prefix = bytes => {
  const length = Buffer.alloc(4)
  length.writeUInt32BE(bytes.length)
  return concat(length, bytes)
}
const transcript = await hash(concat(prefix(client), prefix(server)))
const keyMaterial = await subtle.importKey('raw', shared, 'HKDF', false, [
  'deriveKey',
])

async function record(direction, label, plaintext) {
  const key = await subtle.deriveKey(
    {
      name: 'HKDF',
      hash: 'SHA-256',
      salt: transcript,
      info: utf8(`Start-RPC-Session v1\0${label}`),
    },
    keyMaterial,
    { name: 'AES-GCM', length: 256 },
    false,
    ['encrypt'],
  )
  const data = await subtle.encrypt(
    {
      name: 'AES-GCM',
      iv: new Uint8Array(12),
      additionalData: concat(utf8('Start-RPC-Session v1\0'), [direction]),
      tagLength: 128,
    },
    key,
    utf8(plaintext),
  )
  return { plaintext, ciphertext: b64(data) }
}

const fixture = {
  context,
  clientPrivate,
  serverPrivate,
  clientPublic: b64(clientDh.public),
  serverPublic: b64(serverDh.public),
  clientIdentity: b64(clientIdentity.publicDer),
  serverIdentity: b64(serverIdentity.publicDer),
  client: b64(client),
  server: b64(server),
  clientRecord: await record(0, 'client-to-server', 'client request'),
  serverRecord: await record(1, 'server-to-client', 'server reply'),
}
await writeFile(
  new URL('webcrypto.json', import.meta.url),
  JSON.stringify(fixture, null, 2) + '\n',
)
console.log(
  `WebCrypto fixture: client ${client.length} bytes, server ${server.length} bytes`,
)
