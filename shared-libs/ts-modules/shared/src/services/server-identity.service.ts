import { inject, Service } from '@angular/core'
import { WA_WINDOW } from '@ng-web-apis/common'
import { TuiNotificationService } from '@taiga-ui/core'

import { AUTH_KEY_STORAGE_KEY } from './auth-key.service'
import { identityStore } from './identity-store'
import { RELATIVE_URL } from '../tokens/relative-url'
import type { SessionOpenEvent } from './session-websocket'

@Service()
export class ServerIdentityService {
  private readonly win = inject(WA_WINDOW)
  private readonly relativeUrl = inject(RELATIVE_URL)
  private readonly storageKey = inject(AUTH_KEY_STORAGE_KEY)
  private readonly alerts = inject(TuiNotificationService)
  readonly openObserver = {
    next: (event: Event) => {
      void this.observe((event as SessionOpenEvent).peer)
    },
  }

  async observe(peer: string): Promise<void> {
    const endpoint = new URL(this.relativeUrl, this.win.location.origin)
    if (endpoint.origin !== this.win.location.origin) return
    const target = new URL(endpoint.pathname, endpoint.origin).href
    const key = `${this.storageKey}/serverIdentity/${target}`
    try {
      const { verifyingKeyFingerprint } = await import('./session-identity')
      const current = await verifyingKeyFingerprint(peer)
      const previous = await identityStore<string | undefined>(
        this.win,
        'readwrite',
        store => {
          const request = store.get(key)
          request.onsuccess = () => {
            if (request.result !== current) store.put(current, key)
          }
          return request
        },
      )
      if (previous && previous !== current) {
        const { SERVER_IDENTITY_WARNING } =
          await import('../components/server-identity')
        this.alerts
          .open(SERVER_IDENTITY_WARNING, {
            appearance: 'warning',
            autoClose: 0,
            data: { target, previous, current },
          })
          .subscribe()
      }
    } catch (error) {
      console.warn('Server identity observation failed', error)
    }
  }
}
