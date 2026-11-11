import '@angular/compiler'
import { DOCUMENT, Injector } from '@angular/core'
import { WA_WINDOW } from '@ng-web-apis/common'
import { TuiNotificationService } from '@taiga-ui/core'
import { auth } from '@start9labs/start-core'
import { SESSION_PROFILE } from '@start9labs/start-core/osBindings/session-profile'
import {
  AuthKeyService,
  AUTH_KEY_STORAGE_KEY,
  AUTH_SIGNING_HOST,
} from '../src/services/auth-key.service'
import { RpcSocketService } from '../src/services/rpc-socket.service'
import { ServerIdentityService } from '../src/services/server-identity.service'
import { sessionWebSocketConstructor } from '../src/services/session-websocket'
import { RELATIVE_URL } from '../src/tokens/relative-url'

const NativeSocket = WebSocket
let tamper = false
class ObservedSocket extends NativeSocket {
  private sends = 0
  override send(data: Parameters<WebSocket['send']>[0]) {
    this.sends++
    if (tamper && this.sends === 2 && ArrayBuffer.isView(data)) {
      tamper = false
      const changed = new Uint8Array(
        data.buffer,
        data.byteOffset,
        data.byteLength,
      ).slice()
      changed[changed.length - 1] = changed[changed.length - 1]! ^ 1
      super.send(changed)
    } else super.send(data)
  }
}
Object.defineProperty(globalThis, 'WebSocket', {
  value: ObservedSocket,
  configurable: true,
})
const injector = Injector.create({
  providers: [
    AuthKeyService,
    RpcSocketService,
    ServerIdentityService,
    { provide: DOCUMENT, useValue: document },
    { provide: WA_WINDOW, useValue: window },
    { provide: AUTH_KEY_STORAGE_KEY, useValue: 'signed-session-probe' },
    { provide: AUTH_SIGNING_HOST, useValue: location.hostname },
    { provide: RELATIVE_URL, useValue: '/rpc/v0' },
    {
      provide: TuiNotificationService,
      useValue: {
        open() {
          throw Error('Unexpected identity rotation')
        },
      },
    },
  ],
})
const keys = injector.get(AuthKeyService)
const rpc = injector.get(RpcSocketService)
const identities = injector.get(ServerIdentityService)
const peers: string[] = []
const observe = identities.observe.bind(identities)
identities.observe = async peer => {
  peers.push(peer)
  await observe(peer)
}
let continuation: WebSocket | undefined
let nextRevision: Promise<unknown> | undefined
const body = (method: string, params: object = {}) =>
  JSON.stringify({ method, params })
async function call(method: string, params: object = {}) {
  const lease = await keys.lease()
  if (!lease) throw Error('No key')
  try {
    return await rpc.message(
      body(method, params),
      { openObserver: identities.openObserver },
      lease,
    )
  } catch (error) {
    const cause = (error as Error).cause as ErrorEvent | undefined
    console.error('RPC cause', cause?.error?.stack ?? cause)
    throw error
  }
}
const probe = {
  async enroll() {
    const key = await keys.create()
    await fetch('/probe/enroll', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(key.pubkeyPem),
    })
    return key.privateKey.extractable
  },
  async bulk() {
    const response = await call('info')
    return (response as { result: { name: string } }).result.name.length
  },
  async sequence() {
    const ids = []
    for (let i = 0; i < 8; i++) {
      const response = await call('info')
      ids.push((response as { id: string }).id)
    }
    return ids
  },
  async subscribe() {
    const response = await call('db.subscribe', { pointer: '/index' })
    const { result } = response as {
      result: {
        dump: { value: { package: { packages: object } } }
        guid: string
      }
    }
    if (!('secret' in result.dump.value.package.packages))
      throw Error('Reader enrollment was not live')
    continuation = rpc.openSocket(result.guid)
    continuation.addEventListener('open', identities.openObserver.next, {
      once: true,
    })
    await new Promise((resolve, reject) => {
      continuation!.onopen = resolve
      continuation!.onerror = reject
    })
    nextRevision = new Promise((resolve, reject) => {
      continuation!.onmessage = event => resolve(JSON.parse(event.data))
      continuation!.onerror = reject
    })
    return result.guid
  },
  async revision() {
    const revision = (await nextRevision) as { patch: { value?: string }[] }
    const name = revision.patch.find(
      item => typeof item.value === 'string',
    )!.value!
    continuation!.close()
    return { length: name.length, bom: name.charCodeAt(0) }
  },
  async numeric() {
    const lease = (await keys.lease())!
    const Constructor = sessionWebSocketConstructor(
      async () => ({
        signer: auth.requestSigner(lease.key),
        signal: lease.signal,
      }),
      location.hostname,
      SESSION_PROFILE.identity.rpcTarget,
    )
    const socket = new Constructor(`ws://${location.host}/rpc/v2`)
    await new Promise((resolve, reject) => {
      socket.onopen = resolve
      socket.onerror = reject
    })
    const response = new Promise<string>((resolve, reject) => {
      socket.onmessage = event => resolve(event.data)
      socket.onerror = reject
    })
    socket.send(
      '{"jsonrpc":"2.0","id":18446744073709551615,"method":"info","params":{}}',
    )
    const result = await response
    socket.close()
    return result.includes('"id":18446744073709551615')
  },
  async identity() {
    const lease = (await keys.lease())!
    return {
      extractable: lease.key.privateKey.extractable,
      fingerprint: await auth.verifyingKeyFingerprint(peers[0]!),
    }
  },
  async tamper() {
    rpc.disconnect()
    tamper = true
    try {
      await call('info')
      throw Error('Tampering succeeded')
    } catch (error) {
      if ((error as { code?: number }).code !== 0) throw error
      return true
    }
  },
  async revoke() {
    await keys.clear()
    return (await keys.lease()) === null
  },
}
Object.assign(window, { sessionProbe: probe })
