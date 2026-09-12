import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { SwUpdate } from '@angular/service-worker'
import { WA_WINDOW } from '@ng-web-apis/common'
import { ErrorService, i18nPipe } from '@start9labs/shared'
import { Version } from '@start9labs/start-core'
import { TuiResponsiveDialog } from '@taiga-ui/addon-mobile'
import { TuiButton } from '@taiga-ui/core'
import { TuiNotificationMiddleService } from '@taiga-ui/kit'
import { PatchDB } from 'patch-db-client'
import { distinctUntilChanged, map, merge, Subject } from 'rxjs'
import { ConfigService } from 'src/app/services/config.service'
import { DataModel } from 'src/app/services/patch-db/data-model'

@Component({
  selector: 'refresh-alert',
  template: `
    <ng-template
      [tuiResponsiveDialog]="show()"
      [tuiResponsiveDialogOptions]="{
        label: i18n.transform('Refresh Needed'),
        size: 's',
      }"
      (tuiResponsiveDialogChange)="dismiss$.next()"
    >
      @if (isPwa) {
        <p>
          {{
            'Your user interface is cached and out of date. Attempt to reload the PWA using the button below. If you continue to see this message, uninstall and reinstall the PWA.'
              | i18n
          }}
        </p>
      } @else {
        <p>
          {{
            'StartOS has been updated, but this page is still running the previous interface. Reload the page to get the latest version.'
              | i18n
          }}
        </p>
      }
      <button
        tuiButton
        appearance="secondary"
        style="float: right"
        [tuiAppearanceFocus]="false"
        (click)="reload()"
      >
        {{ (isPwa ? 'Refresh' : 'Reload') | i18n }}
      </button>
    </ng-template>
  `,
  imports: [TuiResponsiveDialog, TuiButton, i18nPipe],
})
export class RefreshAlertComponent {
  private readonly win = inject(WA_WINDOW)
  private readonly updates = inject(SwUpdate)
  private readonly loader = inject(TuiNotificationMiddleService)
  private readonly error = inject(ErrorService)
  private readonly version = Version.parse(inject(ConfigService).version)

  readonly i18n = inject(i18nPipe)

  readonly dismiss$ = new Subject<void>()
  readonly isPwa = this.win.matchMedia('(display-mode: standalone)').matches

  readonly show = toSignal(
    merge(
      this.dismiss$.pipe(map(() => false)),
      inject<PatchDB<DataModel>>(PatchDB)
        .watch$('serverInfo', 'version')
        .pipe(
          distinctUntilChanged(),
          map(v => this.version.compare(Version.parse(v)) !== 'equal'),
        ),
    ),
    {
      initialValue: false,
    },
  )

  protected async reload(): Promise<void> {
    const loader = this.isPwa
      ? this.loader.open(this.i18n.transform('Reloading PWA')).subscribe()
      : undefined

    try {
      if (
        this.updates.isEnabled &&
        this.win.navigator.serviceWorker.controller !== null
      ) {
        await this.updates.checkForUpdate()
        await this.updates.activateUpdate()
      }
    } catch (e: any) {
      this.error.handleError(e)
      return
    } finally {
      loader?.unsubscribe()
    }

    this.win.location.reload()
  }
}
