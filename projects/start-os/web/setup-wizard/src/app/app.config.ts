import {
  provideHttpClient,
  withFetch,
  withInterceptorsFromDi,
} from '@angular/common/http'
import { ApplicationConfig, provideZoneChangeDetection } from '@angular/core'
import {
  PreloadAllModules,
  provideRouter,
  withDisabledInitialNavigation,
  withPreloading,
} from '@angular/router'
import {
  I18N_PROVIDERS,
  provideSetupLogsService,
  RELATIVE_URL,
  VERSION,
  WorkspaceConfig,
} from '@start9labs/shared'
import {
  tuiButtonOptionsProvider,
  provideTaiga,
  tuiHintOptionsProvider,
  tuiDialogOptionsProvider,
} from '@taiga-ui/core'

import { ROUTES } from './app.routes'
import { ApiService } from './services/api.service'
import { LiveApiService } from './services/live-api.service'
import { MockApiService } from './services/mock-api.service'

const {
  useMocks,
  ui: { api },
} = require('../../../../../../config.json') as WorkspaceConfig

const version = require('../../../../../../package.json').version

export const APP_CONFIG: ApplicationConfig = {
  providers: [
    provideZoneChangeDetection(),
    provideTaiga({ mode: 'dark' }),
    tuiHintOptionsProvider({ appearance: 'primary-grayscale' }),
    tuiDialogOptionsProvider({ size: 's' }),
    provideRouter(
      ROUTES,
      withDisabledInitialNavigation(),
      withPreloading(PreloadAllModules),
    ),
    I18N_PROVIDERS,
    provideSetupLogsService(ApiService),
    tuiButtonOptionsProvider({ size: 'm' }),
    {
      provide: ApiService,
      useClass: useMocks ? MockApiService : LiveApiService,
    },
    {
      provide: RELATIVE_URL,
      useValue: `/${api.url}/${api.version}`,
    },
    {
      provide: VERSION,
      useValue: version,
    },
    provideHttpClient(withInterceptorsFromDi(), withFetch()),
  ],
}
