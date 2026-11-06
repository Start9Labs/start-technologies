import { HttpHeaders } from '@angular/common/http'
import { inject, InjectionToken, Service } from '@angular/core'
import { WA_WINDOW } from '@ng-web-apis/common'
import { auth } from '@start9labs/start-core'

import type { HttpOptions } from '../types/http.types'
import type { RPCOptions } from '../types/rpc.types'
import { rpcRequestBody } from './rpc-request'

/** IndexedDB record key for the device key. Override per app so two apps
 *  served from one origin (dev servers, a re-pointed forward) can't clobber
 *  each other's enrollment. */
export const AUTH_KEY_STORAGE_KEY = new InjectionToken<string>(
  'AUTH_KEY_STORAGE_KEY',
  { factory: () => '_startos/authKey' },
)

/** The server identity requests are signed for. */
export const AUTH_SIGNING_HOST = new InjectionToken<string>(
  'AUTH_SIGNING_HOST',
  { factory: () => inject(WA_WINDOW).location.hostname },
)

const DB_NAME = 'startos-auth'
const STORE = 'keys'

/**
 * Holds the device key this browser enrolled at login: a non-extractable
 * WebCrypto Ed25519 key persisted in IndexedDB, so a script can sign with it
 * while the page lives but can never read it out. Cleared by the app on
 * logout or server rejection; the server-side enrollment is revoked
 * separately.
 */
@Service()
export class AuthKeyService {
  private readonly win = inject(WA_WINDOW)
  private readonly storageKey = inject(AUTH_KEY_STORAGE_KEY)
  private readonly host = inject(AUTH_SIGNING_HOST)
  private cached: auth.AuthKey | null | undefined
  /** Preserves another tab's displaced key across a failed login. */
  private displaced: auth.AuthKey | null = null

  async get(): Promise<auth.AuthKey | null> {
    if (this.cached === undefined) {
      try {
        this.cached =
          (await this.idb('readonly', s => s.get(this.storageKey))) ?? null
      } catch {
        this.cached = null
        await this.delete().catch(() => {})
      }
    }
    return this.cached ?? null
  }

  async create(): Promise<auth.AuthKey> {
    const key = await auth.generateAuthKey()
    this.displaced = await this.get()
    await this.put(key)
    return key
  }

  /** Restores the key displaced by a failed login. */
  async rollback(): Promise<void> {
    const restore = this.displaced
    this.displaced = null
    if (restore) {
      await this.put(restore)
    } else {
      await this.delete()
    }
  }

  /** Destroy the stored key. For logout — a rejected login wants `rollback()`. */
  async clear(): Promise<void> {
    this.displaced = null
    await this.delete()
  }

  async signHeader(
    body: string | Uint8Array,
    headers?: HttpOptions['headers'],
  ): Promise<HttpHeaders> {
    const result =
      headers instanceof HttpHeaders ? headers : new HttpHeaders(headers)
    const key = await this.get()
    if (!key) return result
    const bytes =
      typeof body === 'string' ? new TextEncoder().encode(body) : body
    return result.set(
      auth.AUTH_SIG_HEADER,
      await auth.signRequest(key, this.host, bytes),
    )
  }

  /** Captures a serialized RPC body and adds the stored device key's proof when available. */
  async signRpcBody(
    options: Pick<RPCOptions<unknown>, 'method' | 'params'>,
  ): Promise<string> {
    const body = rpcRequestBody(options)
    const key = await this.get()
    if (!key) return body
    const proof = await auth.signRpcRequest(key, this.host, body)
    // Reparsing would round raw JSON integers and change numeric token types.
    return `{${JSON.stringify(auth.RPC_SIGNATURE_FIELD)}:${JSON.stringify(proof)},${body.slice(1)}`
  }

  private async put(key: auth.AuthKey): Promise<void> {
    this.cached = key
    await this.idb('readwrite', s => s.put(key, this.storageKey))
  }

  private async delete(): Promise<void> {
    this.cached = null
    await this.idb('readwrite', s => s.delete(this.storageKey))
  }

  private idb<T>(
    mode: IDBTransactionMode,
    op: (store: IDBObjectStore) => IDBRequest<T>,
  ): Promise<T> {
    return new Promise((resolve, reject) => {
      const open = this.win.indexedDB.open(DB_NAME, 1)
      open.onupgradeneeded = () => open.result.createObjectStore(STORE)
      open.onerror = () => reject(open.error)
      open.onsuccess = () => {
        const db = open.result
        const tx = db.transaction(STORE, mode)
        const req = op(tx.objectStore(STORE))
        tx.oncomplete = () => {
          db.close()
          resolve(req.result)
        }
        tx.onerror = () => {
          db.close()
          reject(tx.error)
        }
        tx.onabort = () => {
          db.close()
          reject(tx.error)
        }
      }
    })
  }
}
