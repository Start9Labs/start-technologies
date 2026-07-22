import { inject, Injectable } from '@angular/core'
import { WA_LOCAL_STORAGE } from '@ng-web-apis/common'
import { auth } from '@start9labs/start-core'

const STORAGE_KEY = '_startos/authKey'

interface StoredAuthKey {
  secretKey: string
  pubkeyPem: string
}

/**
 * Holds the device key this browser enrolled at login. Persisted in
 * localStorage, so `StorageService.clear()` on logout destroys it along with
 * everything else; the server-side enrollment is revoked separately.
 */
@Injectable({
  providedIn: 'root',
})
export class AuthKeyService {
  private readonly storage = inject(WA_LOCAL_STORAGE)

  get(): auth.AuthKey | null {
    const raw = this.storage?.getItem(STORAGE_KEY)
    if (!raw) return null
    const stored: StoredAuthKey = JSON.parse(raw)
    return {
      secretKey: auth.base64ToBytes(stored.secretKey),
      pubkeyPem: stored.pubkeyPem,
    }
  }

  create(): auth.AuthKey {
    const key = auth.generateAuthKey()
    const stored: StoredAuthKey = {
      secretKey: auth.bytesToBase64(key.secretKey),
      pubkeyPem: key.pubkeyPem,
    }
    this.storage?.setItem(STORAGE_KEY, JSON.stringify(stored))
    return key
  }

  discard(): void {
    this.storage?.removeItem(STORAGE_KEY)
  }

  signHeader(body: string | Uint8Array): Record<string, string> {
    const key = this.get()
    if (!key) return {}
    const bytes =
      typeof body === 'string' ? new TextEncoder().encode(body) : body
    return {
      [auth.AUTH_SIG_HEADER]: auth.signRequest(
        key,
        window.location.hostname,
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
