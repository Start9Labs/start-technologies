import {
  provideHttpClient,
  withFetch,
  withInterceptorsFromDi,
} from '@angular/common/http'
import {
  ApplicationConfig,
  computed,
  inject,
  provideAppInitializer,
  provideZoneChangeDetection,
} from '@angular/core'
import { UntypedFormBuilder } from '@angular/forms'
import {
  ActivationStart,
  PreloadAllModules,
  provideRouter,
  Router,
  withComponentInputBinding,
  withDisabledInitialNavigation,
  withInMemoryScrolling,
  withPreloading,
  withRouterConfig,
} from '@angular/router'
import { provideServiceWorker } from '@angular/service-worker'
import { WA_WINDOW } from '@ng-web-apis/common'
import {
  AbstractMarketplaceService,
  FilterPackagesPipe,
  MARKETPLACE_REGISTRY_ALERTS,
} from '@start9labs/marketplace'
import { z } from '@start9labs/start-core'
import {
  I18N_PROVIDERS,
  I18N_STORAGE,
  i18nService,
  Languages,
  RELATIVE_URL,
  VERSION,
  WorkspaceConfig,
} from '@start9labs/shared'
import {
  tuiObfuscateOptionsProvider,
  tuiProvide,
  tuiWindowSize,
} from '@taiga-ui/cdk'
import {
  provideTaiga,
  TUI_BREAKPOINT,
  TUI_DIALOGS_CLOSE,
  TUI_MEDIA,
  tuiButtonOptionsProvider,
  tuiDateFormatProvider,
  tuiDropdownOptionsProvider,
  tuiHintOptionsProvider,
  tuiNotificationOptionsProvider,
  tuiNumberFormatProvider,
} from '@taiga-ui/core'
import {
  tuiInputDateOptionsProvider,
  tuiInputDateTimeOptionsProvider,
} from '@taiga-ui/kit'
import { PatchDB } from 'patch-db-client'
import { filter, identity, merge, pairwise } from 'rxjs'
import { MarketplaceAlertsService } from 'src/app/routes/portal/routes/marketplace/services/alerts.service'
import { FilterUpdatesPipe } from 'src/app/routes/portal/routes/updates/filter-updates.pipe'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { LiveApiService } from 'src/app/services/api/embassy-live-api.service'
import { MockApiService } from 'src/app/services/api/embassy-mock-api.service'
import { AuthService } from 'src/app/services/auth.service'
import { ClientStorageService } from 'src/app/services/client-storage.service'
import { ConfigService } from 'src/app/services/config.service'
import { MarketplaceService } from 'src/app/services/marketplace.service'
import {
  PATCH_CACHE,
  PatchDbSource,
} from 'src/app/services/patch-db/patch-db-source'
import { PluginsService } from 'src/app/services/plugins.service'
import { StateService } from 'src/app/services/state.service'
import { StorageService } from 'src/app/services/storage.service'
import {
  DatetimeTransformer,
  DateTransformer,
} from 'src/app/utils/value-transformers'
import { environment } from 'src/environments/environment'

import { ROUTES } from './app.routes'

const {
  useMocks,
  ui: { api },
} = require('../../../../../../config.json') as WorkspaceConfig

// The UI is served under a CSP without 'unsafe-eval': make zod take its
// interpreted path instead of probing `new Function()` against the policy.
z.config({ jitless: true })

export const APP_CONFIG: ApplicationConfig = {
  providers: [
    provideZoneChangeDetection(),
    provideTaiga({ mode: 'dark' }),
    provideHttpClient(withInterceptorsFromDi(), withFetch()),
    provideRouter(
      ROUTES,
      withDisabledInitialNavigation(),
      withComponentInputBinding(),
      withPreloading(PreloadAllModules),
      withInMemoryScrolling({ scrollPositionRestoration: 'enabled' }),
      withRouterConfig({ paramsInheritanceStrategy: 'always' }),
    ),
    provideServiceWorker('ngsw-worker.js', {
      enabled: environment.useServiceWorker,
      // Register the ServiceWorker as soon as the application is stable
      // or after 30 seconds (whichever comes first).
      registrationStrategy: 'registerWhenStable:30000',
    }),
    I18N_PROVIDERS,
    FilterPackagesPipe,
    FilterUpdatesPipe,
    UntypedFormBuilder,
    tuiNumberFormatProvider({ decimalSeparator: '.', thousandSeparator: '' }),
    tuiButtonOptionsProvider({ size: 'm' }),
    tuiDropdownOptionsProvider({ appearance: 'start-os' }),
    tuiNotificationOptionsProvider({
      autoClose: appearance => (appearance === 'negative' ? 0 : 3000),
    }),
    tuiDateFormatProvider({ mode: 'mm/dd/yyyy', separator: '/' }),
    tuiInputDateOptionsProvider({ valueTransformer: DateTransformer }),
    tuiInputDateTimeOptionsProvider({ valueTransformer: DatetimeTransformer }),
    tuiHintOptionsProvider({ appearance: 'primary-grayscale' }),
    {
      provide: ApiService,
      useClass: useMocks ? MockApiService : LiveApiService,
    },
    {
      provide: PatchDB,
      deps: [PatchDbSource, PATCH_CACHE],
      useClass: PatchDB,
    },
    tuiProvide(AbstractMarketplaceService, MarketplaceService),
    tuiProvide(MARKETPLACE_REGISTRY_ALERTS, MarketplaceAlertsService),
    provideAppInitializer(() => {
      const i18n = inject(i18nService)
      const clientStorage = inject(ClientStorageService)
      const router = inject(Router)
      const auth = inject(AuthService)

      inject(StorageService).migrate036()
      // Await the (IndexedDB) key check so the guards see the right auth
      // state on the very first navigation.
      return auth.init().then(() => {
        clientStorage.init()
        router.initialNavigation()
        i18n.setLanguage(i18n.language || 'english')
      })
    }),
    {
      provide: RELATIVE_URL,
      useValue: `/${api.url}/${api.version}`,
    },
    {
      provide: TUI_DIALOGS_CLOSE,
      useFactory: () =>
        merge(
          inject(Router).events.pipe(filter(e => e instanceof ActivationStart)),
          inject(StateService).pipe(
            pairwise(),
            filter(
              ([prev, curr]) =>
                prev === 'running' &&
                (curr === 'error' || curr === 'initializing'),
            ),
          ),
        ),
    },
    {
      provide: I18N_STORAGE,
      useFactory: () => {
        const api = inject(ApiService)

        return (language: Languages) => api.setLanguage({ language })
      },
    },
    {
      provide: VERSION,
      useFactory: () => inject(ConfigService).version,
    },
    tuiObfuscateOptionsProvider({
      recipes: {
        mask: ({ length }) => '•'.repeat(length),
        none: identity,
      },
    }),
    {
      provide: TUI_MEDIA,
      useValue: {
        mobile: 1000,
        desktopSmall: 1280,
        desktopLarge: Infinity,
      },
    },
    {
      provide: TUI_BREAKPOINT,
      useFactory: () => {
        const media = inject(TUI_MEDIA)
        const window = tuiWindowSize(inject(WA_WINDOW))
        const plugins = inject(PluginsService)

        return computed(() => {
          const { width } = window()
          const value = plugins.enabled()
            ? 320 + (width - 640) * (plugins.size() / 100)
            : width

          return value < media.mobile ? 'mobile' : 'desktopLarge'
        })
      },
    },
  ],
}
