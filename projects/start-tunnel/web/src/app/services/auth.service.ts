import { inject, Injectable, signal } from '@angular/core'
import { AuthKeyService } from '@start9labs/shared'

@Injectable({
  providedIn: 'root',
})
export class AuthService {
  private readonly authKeys = inject(AuthKeyService)

  readonly authenticated = signal(Boolean(this.authKeys.get()))

  deauthenticate(): void {
    this.authKeys.discard()
    this.authenticated.set(false)
  }
}
