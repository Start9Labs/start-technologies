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
  signal,
} from '@angular/core'
import {
  PreloadAllModules,
  provideRouter,
  withPreloading,
  withRouterConfig,
} from '@angular/router'
import { tuiSheetDialogOptionsProvider } from '@taiga-ui/addon-mobile'
import {
  provideTaiga,
  tuiButtonOptionsProvider,
  tuiCheckboxOptionsProvider,
  tuiDialogOptionsProvider,
  tuiIconsProvider,
  tuiRadioOptionsProvider,
  tuiTextfieldOptionsProvider,
} from '@taiga-ui/core'
import { tuiBadgeOptionsProvider, tuiTabsOptionsProvider } from '@taiga-ui/kit'
import {
  tuiCardOptionsProvider,
  tuiFormOptionsProvider,
} from '@taiga-ui/layout'
import { ICONS } from 'src/app/app.icons'

import { I18N_PROVIDERS } from 'src/app/i18n/i18n.providers'
import { i18nService } from 'src/app/i18n/i18n.service'

import { routes } from './app.routes'
import { ApiService } from './services/api/api.service'
import { LiveApiService } from './services/api/live-api.service'
import { MockApiService } from './services/api/mock-api.service'
import { AuthService } from './services/auth.service'
import { RELATIVE_URL } from '@start9labs/shared'
import { GIT_HASH, IS_MOCK, WorkspaceConfig } from './utils/workspace-config'

const { useMocks, api, gitHash } =
  require('../../config.json') as WorkspaceConfig

export const appConfig: ApplicationConfig = {
  providers: [
    provideBrowserGlobalErrorListeners(),
    provideZonelessChangeDetection(),
    // Preload every lazy route chunk in the background once the app boots (while
    // the connection is healthy), mirroring start-os. This avoids an on-demand
    // import() during a network restart, which would fail against the dead
    // connection and get cached as an errored module — leaving that route
    // permanently un-loadable until a hard refresh.
    provideRouter(
      routes,
      withPreloading(PreloadAllModules),
      withRouterConfig({ onSameUrlNavigation: 'reload' }),
    ),
    provideTaiga({ scrollbars: 'native' }),
    tuiIconsProvider(ICONS),
    tuiButtonOptionsProvider({ size: 'm' }),
    tuiBadgeOptionsProvider({ size: 'm' }),
    tuiTabsOptionsProvider({ size: 'm' }),
    tuiRadioOptionsProvider({ size: 's' }),
    tuiCheckboxOptionsProvider({ size: 's' }),
    tuiTextfieldOptionsProvider({ size: signal('m'), cleaner: signal(false) }),
    tuiFormOptionsProvider({ size: 'm' }),
    tuiCardOptionsProvider({ space: 'compact', appearance: 'floating' }),
    tuiDialogOptionsProvider({ size: 's' }),
    tuiSheetDialogOptionsProvider({ bar: false, offset: 72 }),
    {
      provide: ApiService,
      useClass: useMocks ? MockApiService : LiveApiService,
    },
    {
      provide: IS_MOCK,
      useValue: useMocks,
    },
    {
      provide: GIT_HASH,
      useValue: gitHash,
    },
    {
      provide: RELATIVE_URL,
      useValue: `/${api.url}/${api.version}`,
    },
    provideHttpClient(withInterceptorsFromDi(), withFetch()),
    provideAppInitializer(() => inject(AuthService).whenReady()),
    ...I18N_PROVIDERS,
    // Apply the language the Taiga switcher remembers at boot (pre-auth screens);
    // the authenticated shell (app.ts) later applies the backend-stored value.
    provideAppInitializer(() => {
      const i18n = inject(i18nService)
      i18n.setLangLocal(i18n.lang)
    }),
  ],
}
