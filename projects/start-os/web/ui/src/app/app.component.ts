import { Component, inject } from '@angular/core'
import { takeUntilDestroyed } from '@angular/core/rxjs-interop'
import { Meta } from '@angular/platform-browser'
import { RouterOutlet } from '@angular/router'
import { i18nService } from '@start9labs/shared'
import { TuiRoot } from '@taiga-ui/core'
import { PatchDB } from 'patch-db-client'
import { merge } from 'rxjs'
import { ToastContainerComponent } from 'src/app/components/toast-container.component'
import { PatchDataService } from './services/patch-data.service'
import { DataModel } from './services/patch-db/data-model'
import { PatchMonitorService } from './services/patch-monitor.service'

@Component({
  selector: 'app-root',
  imports: [TuiRoot, RouterOutlet, ToastContainerComponent],
  template: `
    <tui-root>
      <router-outlet />
      <toast-container />
    </tui-root>
  `,
  styles: `
    :host {
      display: block;
      height: 100%;
    }

    tui-root {
      height: 100%;
      font-family: 'Hanken Grotesk', system-ui;
    }
  `,
})
export class AppComponent {
  private readonly i18n = inject(i18nService)
  private readonly meta = inject(Meta)

  readonly subscription = merge(
    inject(PatchDataService),
    inject(PatchMonitorService),
  )
    .pipe(takeUntilDestroyed())
    .subscribe()

  readonly ui = inject<PatchDB<DataModel>>(PatchDB)
    .watch$('serverInfo', 'language')
    .pipe(takeUntilDestroyed())
    .subscribe(language => {
      this.i18n.setLangLocal(language || 'en_US')
    })

  // Safari takes the Home Screen name from this meta tag, not the manifest.
  readonly appName = inject<PatchDB<DataModel>>(PatchDB)
    .watch$('serverInfo', 'hostname')
    .pipe(takeUntilDestroyed())
    .subscribe(hostname => {
      this.meta.updateTag({
        name: 'apple-mobile-web-app-title',
        content: hostname || 'StartOS',
      })
    })
}
