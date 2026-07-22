import { inject, InjectionToken, Service } from '@angular/core'
import { WA_LOCAL_STORAGE, WA_WINDOW } from '@ng-web-apis/common'
import { auth } from '@start9labs/start-core'

/** localStorage slot for the device key. Override per app so two apps served
 *  from one origin (dev servers, a re-pointed forward) can't clobber each
 *  other's enrollment. */
export const AUTH_KEY_STORAGE_KEY = new InjectionToken<string>(
  'AUTH_KEY_STORAGE_KEY',
  { factory: () => '_startos/authKey' },
)

interface StoredAuthKey {
  secretKey: string
  pubkeyPem: string
}

/**
 * Holds the device key this browser enrolled at login. Persisted in
 * localStorage until the app clears it — on logout or on a server rejection;
 * the server-side enrollment is revoked separately.
 */
@Service()
export class AuthKeyService {
  private readonly storage = inject(WA_LOCAL_STORAGE)
  private readonly win = inject(WA_WINDOW)
  private readonly storageKey = inject(AUTH_KEY_STORAGE_KEY)
  /** Raw slot value displaced by this tab's `create()`, restored by
   *  `rollback()` so a failed login here can't wipe the key another tab is
   *  actively signing with. */
  private displaced: string | null = null

  get(): auth.AuthKey | null {
    const raw = this.storage?.getItem(this.storageKey)
    if (!raw) return null
    try {
      const stored: StoredAuthKey = JSON.parse(raw)
      return {
        secretKey: auth.base64ToBytes(stored.secretKey),
        pubkeyPem: stored.pubkeyPem,
      }
    } catch {
      // A corrupt slot must degrade to logged-out, not brick the app.
      this.storage?.removeItem(this.storageKey)
      return null
    }
  }

  create(): auth.AuthKey {
    const key = auth.generateAuthKey()
    const stored: StoredAuthKey = {
      secretKey: auth.bytesToBase64(key.secretKey),
      pubkeyPem: key.pubkeyPem,
    }
    this.displaced = this.storage?.getItem(this.storageKey) ?? null
    this.storage?.setItem(this.storageKey, JSON.stringify(stored))
    return key
  }

  /** Roll back `create()` after a failed login: restore whatever the slot held
   *  before, so a mistyped password in one tab can't sign out the others. */
  rollback(): void {
    if (this.displaced === null) {
      this.storage?.removeItem(this.storageKey)
    } else {
      this.storage?.setItem(this.storageKey, this.displaced)
    }
    this.displaced = null
  }

  /** Destroy the stored key. For logout — a rejected login wants `rollback()`. */
  clear(): void {
    this.displaced = null
    this.storage?.removeItem(this.storageKey)
  }

  signHeader(body: string | Uint8Array): Record<string, string> {
    const key = this.get()
    if (!key) return {}
    const bytes =
      typeof body === 'string' ? new TextEncoder().encode(body) : body
    return {
      [auth.AUTH_SIG_HEADER]: auth.signRequest(
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
  }): Record<string, string> {
    return this.signHeader(
      JSON.stringify({ method: options.method, params: options.params }),
    )
  }
}
