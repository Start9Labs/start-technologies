import { HttpHeaders } from '@angular/common/http'
import { inject, InjectionToken, Service } from '@angular/core'
import { WA_WINDOW } from '@ng-web-apis/common'
import { auth } from '@start9labs/start-core'

import type { HttpOptions } from '../types/http.types'
import { identityStore } from './identity-store'
import type { SessionSocketIdentity } from './session-websocket'

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

/** The active device identity and its cancellation lifetime. */
export interface AuthKeyLease extends SessionSocketIdentity {
  readonly key: auth.AuthKey
}

/**
 * Holds the device key this browser enrolled at login: a non-extractable
 * WebCrypto Ed25519 key persisted in IndexedDB, so a script can sign with it
 * while the page lives but can never read it out. Cleared by the app on
 * logout or server rejection; the server-side enrollment is revoked
 * separately. Key acquisition and enrollment cannot adopt a replacement identity
 * after their cancellation lifetime ends.
 */
@Service()
export class AuthKeyService {
  private readonly win = inject(WA_WINDOW)
  private readonly storageKey = inject(AUTH_KEY_STORAGE_KEY)
  private readonly host = inject(AUTH_SIGNING_HOST)
  private cached: auth.AuthKey | null | undefined
  private lifetime = new AbortController()
  private admissionKey?: Promise<auth.RequestSigner>
  /** Preserves another tab's displaced key across a failed login. */
  private displaced: auth.AuthKey | null = null

  async get(): Promise<auth.AuthKey | null> {
    if (this.cached === undefined) {
      try {
        const stored = await this.idb('readonly', s => s.get(this.storageKey))
        if (this.cached === undefined) this.cached = stored ?? null
      } catch {
        if (this.cached === undefined) await this.delete().catch(() => {})
      }
    }
    return this.cached ?? null
  }

  /** Invalidated when the active device identity changes. */
  async lease(): Promise<AuthKeyLease | null> {
    const lifetime = this.lifetime
    await this.get()
    lifetime.signal.throwIfAborted()
    return this.cached
      ? {
          key: this.cached,
          signer: auth.requestSigner(this.cached),
          signal: lifetime.signal,
        }
      : null
  }

  /** Carries a device key when present; otherwise uses an unpersisted signer. */
  async admissionLease(): Promise<AuthKeyLease | SessionSocketIdentity> {
    const lifetime = this.lifetime
    const stored = await this.get()
    lifetime.signal.throwIfAborted()
    const signer = stored
      ? auth.requestSigner(stored)
      : await (this.admissionKey ??= auth
          .generateEphemeralRequestSigner()
          .catch(error => {
            if (this.lifetime === lifetime) this.admissionKey = undefined
            throw error
          }))
    lifetime.signal.throwIfAborted()
    return {
      signer,
      signal: lifetime.signal,
      ...(stored ? { key: stored } : {}),
    }
  }

  async create(): Promise<auth.AuthKey> {
    const lifetime = this.lifetime
    const key = await auth.generateAuthKey()
    lifetime.signal.throwIfAborted()
    const displaced = await this.get()
    lifetime.signal.throwIfAborted()
    this.displaced = displaced
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
    const lease = await this.lease()
    if (!lease) return result
    const bytes =
      typeof body === 'string' ? new TextEncoder().encode(body) : body
    const proof = await auth.signRequest(lease.key, this.host, bytes)
    lease.signal.throwIfAborted()
    return result.set(auth.AUTH_SIG_HEADER, proof)
  }

  private async put(key: auth.AuthKey): Promise<void> {
    const lifetime = this.replaceLifetime()
    this.cached = key
    await this.idb('readwrite', s => s.put(key, this.storageKey))
    lifetime.signal.throwIfAborted()
  }

  private async delete(): Promise<void> {
    this.replaceLifetime()
    this.cached = null
    await this.idb('readwrite', s => s.delete(this.storageKey))
  }

  private replaceLifetime(): AbortController {
    this.lifetime.abort()
    this.lifetime = new AbortController()
    this.admissionKey = undefined
    return this.lifetime
  }

  private idb<T>(
    mode: IDBTransactionMode,
    op: (store: IDBObjectStore) => IDBRequest<T>,
  ): Promise<T> {
    return identityStore(this.win, mode, op)
  }
}
