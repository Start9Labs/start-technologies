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
    const stored: StoredAuthKey | null = JSON.parse(
      String(this.storage?.getItem(STORAGE_KEY) || null),
    )
    if (!stored) return null
    return {
      secretKey: fromBase64(stored.secretKey),
      pubkeyPem: stored.pubkeyPem,
    }
  }

  create(): auth.AuthKey {
    const key = auth.generateAuthKey()
    const stored: StoredAuthKey = {
      secretKey: toBase64(key.secretKey),
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
}

function toBase64(bytes: Uint8Array): string {
  let binary = ''
  for (const b of bytes) {
    binary += String.fromCharCode(b)
  }
  return btoa(binary)
}

function fromBase64(b64: string): Uint8Array {
  const binary = atob(b64)
  const out = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) {
    out[i] = binary.charCodeAt(i)
  }
  return out
}
