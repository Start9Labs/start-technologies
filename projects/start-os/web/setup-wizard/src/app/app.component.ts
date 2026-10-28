import { Component, DOCUMENT, inject, OnInit } from '@angular/core'
import { Router, RouterOutlet } from '@angular/router'
import { ErrorService } from '@start9labs/shared'
import { TuiRoot } from '@taiga-ui/core'

import { ApiService } from './services/api.service'
import { StateService } from './services/state.service'

@Component({
  selector: 'app-root',
  template: '<tui-root><router-outlet /></tui-root>',
  imports: [TuiRoot, RouterOutlet],
})
export class AppComponent implements OnInit {
  private readonly api = inject(ApiService)
  private readonly errorService = inject(ErrorService)
  private readonly router = inject(Router)
  private readonly stateService = inject(StateService)
  private readonly document = inject(DOCUMENT)

  async ngOnInit() {
    try {
      this.stateService.kiosk = ['localhost', '127.0.0.1'].includes(
        this.document.location.hostname,
      )

      await this.api.loadPubKey()

      const status = await this.api.rpc.setup.status({})

      switch (status.status) {
        case 'needs-install':
          await this.router.navigate(['/language'])
          break

        case 'incomplete':
          if (status.guid) {
            this.stateService.dataDriveGuid = status.guid
          }
          this.stateService.attach = status.attach
          this.stateService.mokEnrolled = status.mokEnrolled
          this.stateService.osDrive = status.osDrive ?? ''
          await this.router.navigate(['/language'])
          break

        case 'running':
          await this.router.navigate(['/loading'])
          break

        case 'complete':
          await this.router.navigate(['/success'])
          break
      }
    } catch (e: any) {
      this.errorService.handleError(e)
    }
  }
}
