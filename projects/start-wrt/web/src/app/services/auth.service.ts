import { inject, Injectable, signal } from '@angular/core'
import { Router } from '@angular/router'
import { AuthKeyService } from '@start9labs/shared'
import { ApiService } from './api/api.service'

@Injectable({
  providedIn: 'root',
})
export class AuthService {
  private readonly api = inject(ApiService)
  private readonly authKeys = inject(AuthKeyService)
  private readonly router = inject(Router)

  readonly authenticated = signal(false)

  /** Whether the device has been initialized (admin password set).
   *  Defaults to true to avoid flash-of-setup on already-initialized devices. */
  readonly initialized = signal(true)

  /** Whether the device is in setup mode (booted from microSD). */
  readonly setupMode = signal(false)

  /** Disk state from setup status check. */
  readonly setupDisk = signal<{
    emmcFound: boolean
    hasFirmware: boolean
  } | null>(null)

  private readonly ready: Promise<void>

  constructor() {
    this.ready = Promise.all([
      this.authKeys.get().then(key => this.authenticated.set(Boolean(key))),
      this.api
        .checkInitialized()
        .then(res => this.initialized.set(res.initialized))
        .catch(() => {}),
      this.api
        .setupStatus()
        .then(res => {
          this.setupMode.set(res.setupMode)
          this.setupDisk.set(res.disk)
        })
        .catch(() => this.setupMode.set(false)),
    ]).then(() => {})
  }

  /** Resolves once the initialization check has completed. */
  whenReady(): Promise<void> {
    return this.ready
  }

  setUnverified(): void {
    this.authKeys.clear()
    this.authenticated.set(false)
    this.router.navigate(['.'])
  }
}
