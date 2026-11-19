import { Component, inject } from '@angular/core'
import {
  InitializingComponent,
  provideSetupLogsService,
} from '@start9labs/shared'
import { StartupApiService } from 'src/app/services/api/startup-api.service'
import { InitializingProgress } from './service'

@Component({
  template: '<app-initializing [progress]="initialization.progress()" />',
  providers: [
    InitializingProgress,
    provideSetupLogsService(StartupApiService, (api, options) =>
      api.streams.init.logs.follow({}, options),
    ),
  ],
  styles: ':host { height: 100%; }',
  imports: [InitializingComponent],
})
export default class InitializingPage {
  protected readonly initialization = inject(InitializingProgress)
}
