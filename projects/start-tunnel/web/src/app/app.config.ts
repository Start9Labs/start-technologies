import {
  provideHttpClient,
  withFetch,
  withInterceptorsFromDi,
} from '@angular/common/http'
import {
  ApplicationConfig,
  inject,
  provideAppInitializer,
  provideBrowserGlobalErrorListeners,
  provideZonelessChangeDetection,
} from '@angular/core'
import { provideRouter, TitleStrategy, withRouterConfig } from '@angular/router'
import {
  i18nService,
  Languages,
  RELATIVE_URL,
  WorkspaceConfig,
} from '@start9labs/shared'
import { LANG_STORAGE_KEY, TUNNEL_I18N_PROVIDERS } from './i18n/i18n.providers'
import {
  provideTaiga,
  tuiDialogOptionsProvider,
  tuiHintOptionsProvider,
} from '@taiga-ui/core'
import { PatchDB } from 'patch-db-client'
import {
  PATCH_CACHE,
  PatchDbSource,
} from 'src/app/services/patch-db/patch-db-source'
import { AppTitleStrategy } from 'src/app/services/title.service'
import { routes } from './app.routes'
import { ApiService } from './services/api/api.service'
import { LiveApiService } from './services/api/live-api.service'
import { MockApiService } from './services/api/mock-api.service'

const {
  useMocks,
  ui: { api },
} = require('../../../../../config.json') as WorkspaceConfig

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideZonelessChangeDetection(),
    provideRouter(routes, withRouterConfig({ onSameUrlNavigation: 'reload' })),
    provideTaiga({ mode: 'dark' }),
    TUNNEL_I18N_PROVIDERS,
    // Restore the saved language on boot (StartTunnel persists it in localStorage).
    provideAppInitializer(() => {
      inject(i18nService).setLangLocal(
        (localStorage.getItem(LANG_STORAGE_KEY) as Languages) || 'en_US',
      )
    }),
    tuiHintOptionsProvider({ appearance: 'primary-grayscale' }),
    tuiDialogOptionsProvider({ size: 's' }),
    {
      provide: PatchDB,
      deps: [PatchDbSource, PATCH_CACHE],
      useClass: PatchDB,
    },
    {
      provide: ApiService,
      useClass: useMocks ? MockApiService : LiveApiService,
    },
    {
      provide: RELATIVE_URL,
      useValue: `/${api.url}/${api.version}`,
    },
    {
      provide: TitleStrategy,
      useClass: AppTitleStrategy,
    },
    provideHttpClient(withInterceptorsFromDi(), withFetch()),
  ],
}
