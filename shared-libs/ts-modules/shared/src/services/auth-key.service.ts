import { inject, InjectionToken, Service } from '@angular/core'
import { WA_LOCAL_STORAGE, WA_WINDOW } from '@ng-web-apis/common'
import { auth } from '@start9labs/start-core'

/** localStorage slot for the id (public key PEM) of the active device key.
 *  Override per app so two apps served from one origin (dev servers, a
 *  re-pointed forward) can't clobber each other's enrollment. */
export const AUTH_KEY_STORAGE_KEY = new InjectionToken<string>(
  'AUTH_KEY_STORAGE_KEY',
  { factory: () => '_startos/authKey' },
)

const DB_NAME = 'startos-auth'
const STORE = 'keys'

/**
 * Holds the device key this browser enrolled at login. The non-extractable
 * WebCrypto Ed25519 key lives in IndexedDB — a script can sign with it while
 * the page lives, but can never read it out — while its id (the public key
 * PEM) lives in localStorage. A private tab that refuses IndexedDB keeps the
 * key in memory instead: login still works there, for the life of the tab.
 * Cleared by the app on logout or server rejection; the server-side
 * enrollment is revoked separately.
 */
@Service()
export class AuthKeyService {
  private readonly storage = inject(WA_LOCAL_STORAGE)
  private readonly win = inject(WA_WINDOW)
  private readonly storageKey = inject(AUTH_KEY_STORAGE_KEY)
  private cached: auth.AuthKey | null | undefined
  /** Key id displaced by this tab's `create()`, restored by `rollback()` so a
   *  failed login here can't wipe the key another tab is signing with. */
  private displaced: string | null = null

  async get(): Promise<auth.AuthKey | null> {
    if (this.cached !== undefined) return this.cached
    const pem = this.storage?.getItem(this.storageKey)
    if (!pem) {
      this.cached = null
      return null
    }
    const privateKey = await this.idb('readonly', s =>
      s.get(this.record(pem)),
    ).catch(() => undefined)
    this.cached = privateKey ? { privateKey, pubkeyPem: pem } : null
    if (!this.cached) {
      // The id points at nothing (private-tab refresh, cleared site data) —
      // degrade to logged-out.
      this.storage?.removeItem(this.storageKey)
    }
    return this.cached
  }

  async create(): Promise<auth.AuthKey> {
    const key = await auth.generateAuthKey()
    // A private tab can refuse IndexedDB: keep the key in memory so login
    // still works there, for the life of the tab.
    await this.idb('readwrite', s =>
      s.put(key.privateKey, this.record(key.pubkeyPem)),
    ).catch(() => {})
    this.displaced = this.storage?.getItem(this.storageKey) ?? null
    this.storage?.setItem(this.storageKey, key.pubkeyPem)
    this.cached = key
    return key
  }

  /** Roll back `create()` after a failed login: restore whatever key id the
   *  slot held before, so a mistyped password in one tab can't sign out the
   *  others. */
  async rollback(): Promise<void> {
    const created = this.storage?.getItem(this.storageKey)
    if (this.displaced === null) {
      this.storage?.removeItem(this.storageKey)
    } else {
      this.storage?.setItem(this.storageKey, this.displaced)
    }
    this.displaced = null
    this.cached = undefined
    if (created) {
      await this.idb('readwrite', s => s.delete(this.record(created))).catch(
        () => {},
      )
    }
  }

  /** Destroy every stored key for this app. For logout — a rejected login
   *  wants `rollback()`. */
  async clear(): Promise<void> {
    this.displaced = null
    this.cached = null
    this.storage?.removeItem(this.storageKey)
    await this.idb('readwrite', s =>
      s.delete(IDBKeyRange.bound(this.record(''), this.record('\uffff'))),
    ).catch(() => {})
  }

  async signHeader(body: string | Uint8Array): Promise<Record<string, string>> {
    const key = await this.get()
    if (!key) return {}
    const bytes =
      typeof body === 'string' ? new TextEncoder().encode(body) : body
    return {
      [auth.AUTH_SIG_HEADER]: await auth.signRequest(
        key,
        this.win.location.hostname,
        bytes,
      ),
    }
  }

  /** Sign an RPC request. The signed bytes must match the body `HttpService`
   *  serializes — `{ method, params }`, in this order, via `JSON.stringify`. */
  signRpcHeaders(options: {
    method: string
    params: unknown
  }): Promise<Record<string, string>> {
    return this.signHeader(
      JSON.stringify({ method: options.method, params: options.params }),
    )
  }

  private record(pem: string): string {
    return `${this.storageKey}/${pem}`
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
