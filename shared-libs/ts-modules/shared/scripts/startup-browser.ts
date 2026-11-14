import '@angular/compiler'
import { DOCUMENT, Injector } from '@angular/core'
import { WA_WINDOW } from '@ng-web-apis/common'
import { firstValueFrom } from 'rxjs'
import type { T } from '@start9labs/start-core'
import { StartupLiveApiService } from '../../../../projects/start-os/web/ui/src/app/services/api/startup-api.service'
import {
  AUTH_KEY_STORAGE_KEY,
  AUTH_SIGNING_HOST,
  AuthKeyService,
} from '../src/services/auth-key.service'
import { identityStore } from '../src/services/identity-store'
import { RpcSocketService } from '../src/services/rpc-socket.service'
import { ServerIdentityService } from '../src/services/server-identity.service'
import { RELATIVE_URL } from '../src/tokens/relative-url'

const injector = Injector.create({
  providers: [
    AuthKeyService,
    RpcSocketService,
    StartupLiveApiService,
    { provide: WA_WINDOW, useValue: window },
    { provide: DOCUMENT, useValue: document },
    { provide: RELATIVE_URL, useValue: '/rpc/v1' },
    { provide: AUTH_SIGNING_HOST, useValue: location.hostname },
    { provide: AUTH_KEY_STORAGE_KEY, useValue: 'startup-browser/key' },
    {
      provide: ServerIdentityService,
      useFactory: () => {
        throw Error('Startup requested normal identity history')
      },
    },
  ],
})
const keys = injector.get(AuthKeyService)
const api = injector.get(StartupLiveApiService)

Object.assign(window, {
  startupProbe: async () => {
    const before = await keys.admissionLease()
    const states = await Promise.all(
      Array.from({ length: 4 }, () => api.rpc.state({})),
    )
    const subscription = await api.rpc.init.subscribe({})
    if (subscription.guid === null)
      throw Error('Ordinary init response omitted its GUID')
    const progress = await firstValueFrom(
      api.openWebsocket$<T.FullProgress>(subscription.guid),
    )
    const enrolled = await keys.lease()
    await keys.clear()
    const after = await keys.admissionLease()
    const fresh = await api.rpc.state({})
    return {
      secure: isSecureContext,
      subtle: typeof crypto.subtle,
      randomUUID: typeof crypto.randomUUID,
      states,
      fresh,
      progress: 'overall' in progress,
      initialBytes: subscription.progress.phases[0]?.name.length,
      continuationBytes: progress.phases[0]?.name.length,
      enrolled,
      canceled: before.signal.aborted,
      freshIdentity: before.signer !== after.signer && !after.signal.aborted,
      records: await identityStore(window, 'readonly', store =>
        store.getAllKeys(),
      ),
    }
  },
})
