import { inject, Injectable, NgZone, signal } from '@angular/core'
import { Router } from '@angular/router'
import { AuthKeyService, i18nKey } from '@start9labs/shared'
import { distinctUntilChanged, map, ReplaySubject } from 'rxjs'
import { StorageService } from './storage.service'

export enum AuthState {
  UNVERIFIED,
  VERIFIED,
}
@Injectable({
  providedIn: 'root',
})
export class AuthService {
  private readonly storage = inject(StorageService)
  private readonly zone = inject(NgZone)
  private readonly router = inject(Router)
  private readonly authKeys = inject(AuthKeyService)
  private readonly authState$ = new ReplaySubject<AuthState>(1)
  private freshLoginAt = 0

  /** Set when a just-established session is rejected (RPC 34 within seconds of
   *  login) — the login page displays it instead of silently bouncing. */
  readonly loginError = signal<i18nKey | null>(null)

  readonly isVerified$ = this.authState$.pipe(
    map(state => state === AuthState.VERIFIED),
    distinctUntilChanged(),
  )

  /** Resolves before initial navigation — routing waits on the key check. */
  async init(): Promise<void> {
    if (await this.authKeys.get()) {
      this.setVerified()
    } else {
      this.setUnverified(true)
    }
  }

  setVerified(freshLogin = false): void {
    if (freshLogin) {
      this.freshLoginAt = Date.now()
    }
    this.authState$.next(AuthState.VERIFIED)
  }

  /** An explicit, user-requested logout — never an error. */
  logout(): void {
    this.freshLoginAt = 0
    this.setUnverified()
  }

  setUnverified(skipNavigation = false): void {
    if (Date.now() - this.freshLoginAt < 10_000) {
      this.loginError.set(
        'Login succeeded, but the server rejected the new device key. Try again.',
      )
    }
    this.freshLoginAt = 0
    this.authState$.next(AuthState.UNVERIFIED)
    // The key lives in IndexedDB, out of reach of `storage.clear()`.
    this.authKeys.clear()
    this.storage.clear()

    if (!skipNavigation) {
      this.zone.run(() => {
        this.router.navigate(['/login'], { replaceUrl: true })
      })
    }
  }
}
