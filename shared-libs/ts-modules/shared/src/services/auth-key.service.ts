import { inject, InjectionToken, Service } from '@angular/core'
import { WA_WINDOW } from '@ng-web-apis/common'
import { auth } from '@start9labs/start-core'

/** IndexedDB record key for the device key. Override per app so two apps
 *  served from one origin (dev servers, a re-pointed forward) can't clobber
 *  each other's enrollment. */
export const AUTH_KEY_STORAGE_KEY = new InjectionToken<string>(
  'AUTH_KEY_STORAGE_KEY',
  { factory: () => '_startos/authKey' },
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
  private cached: auth.AuthKey | null | undefined
  /** Record displaced by this tab's `create()`, restored by `rollback()` so a
   *  failed login here can't wipe the key another tab is signing with. */
  private displaced: auth.AuthKey | null = null

  async get(): Promise<auth.AuthKey | null> {
    if (this.cached === undefined) {
      try {
        this.cached =
          (await this.idb('readonly', s => s.get(this.storageKey))) ?? null
      } catch {
        // A corrupt slot must degrade to logged-out, not brick the app.
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

  /** Roll back `create()` after a failed login: restore whatever the slot held
   *  before, so a mistyped password in one tab can't sign out the others. */
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
