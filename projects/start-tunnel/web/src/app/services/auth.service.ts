import { inject, Service, signal } from '@angular/core'
import { Router } from '@angular/router'
import { AuthKeyService } from '@start9labs/shared'

@Service()
export class AuthService {
  private readonly authKeys = inject(AuthKeyService)
  private readonly router = inject(Router)

  readonly authenticated = signal(Boolean(this.authKeys.get()))

  deauthenticate(): void {
    this.authKeys.clear()
    this.authenticated.set(false)
    // Navigate explicitly — the route guards only re-evaluate on navigation,
    // so a mid-session key rejection would otherwise leave a wedged page.
    this.router.navigate(['/'])
  }
}
