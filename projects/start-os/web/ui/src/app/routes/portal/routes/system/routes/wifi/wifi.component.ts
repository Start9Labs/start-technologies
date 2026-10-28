import { ChangeDetectorRef, Component, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import { RouterLink } from '@angular/router'
import {
  DocsLinkDirective,
  ErrorService,
  i18nKey,
  i18nPipe,
  pauseFor,
  TaskService,
} from '@start9labs/shared'
import { TuiButton, TuiLoader, TuiNotificationService } from '@taiga-ui/core'
import { TuiButtonLoading, TuiSwitch } from '@taiga-ui/kit'
import { TuiCardLarge } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'
import {
  catchError,
  defer,
  exhaustMap,
  finalize,
  first,
  map,
  merge,
  Observable,
  of,
  Subject,
  switchMap,
  takeUntil,
  takeWhile,
  tap,
  timer,
} from 'rxjs'
import {
  FormComponent,
  FormContext,
} from 'src/app/routes/portal/components/form.component'
import { PlaceholderComponent } from 'src/app/routes/portal/components/placeholder.component'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { FormDialogService } from 'src/app/services/form-dialog.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { TitleDirective } from 'src/app/services/title.service'
import { WifiTableComponent } from './table.component'
import { parseWifi, WifiData, WiFiForm } from './utils'
import { wifiSpec } from './wifi.const'

@Component({
  template: `
    <ng-container *title>
      <div>
        <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
          {{ 'Back' | i18n }}
        </a>
        WiFi
        <a
          tuiIconButton
          size="xs"
          docsLink
          path="/start-os/wifi.html"
          appearance="icon"
          iconStart="@tui.book-open-text"
        ></a>
      </div>
    </ng-container>
    <section class="g-card">
      <header>
        Wi-Fi
        <a
          tuiIconButton
          size="xs"
          docsLink
          path="/start-os/wifi.html"
          appearance="icon"
          iconStart="@tui.book-open-text"
        >
          {{ 'Documentation' | i18n }}
        </a>

        @if (status()?.interface) {
          @if (status()?.enabled) {
            <button
              tuiIconButton
              size="s"
              appearance="icon"
              iconStart="@tui.refresh-cw"
              [style.margin-inline-start]="'auto'"
              [loading]="refreshing()"
              (click)="refresh()"
            >
              {{ 'Refresh' | i18n }}
            </button>
          }
          <input
            type="checkbox"
            tuiSwitch
            [style.margin-inline-start]="status()?.enabled ? '' : 'auto'"
            [showIcons]="false"
            [ngModel]="status()?.enabled"
            (ngModelChange)="onToggle($event)"
          />
        }
      </header>

      @if (status()?.interface) {
        @if (status()?.enabled) {
          @if (wifi(); as data) {
            @if (data.known.length) {
              <p class="g-secondary">{{ 'Known Networks' | i18n }}</p>
              <div tuiCardLarge="compact" [wifi]="data.known"></div>
            }
            @if (data.available.length) {
              <p class="g-secondary">{{ 'Other Networks' | i18n }}</p>
              <div tuiCardLarge="compact" [wifi]="data.available"></div>
            }
            <p>
              <button tuiButton (click)="other(data)" appearance="grayscale">
                + {{ 'Connect to hidden network' | i18n }}
              </button>
            </p>
          } @else {
            <tui-loader [style.height.rem]="5" />
          }
        } @else {
          <app-placeholder icon="@tui.wifi">
            {{ 'WiFi is disabled' | i18n }}
          </app-placeholder>
        }
      } @else {
        <app-placeholder icon="@tui.wifi">
          {{ 'No wireless interface detected' | i18n }}
        </app-placeholder>
      }
    </section>
  `,
  styles: `
    :host {
      max-width: 36rem;
    }

    :host-context(tui-root._mobile) header > [docsLink] {
      display: none;
    }
  `,
  imports: [
    FormsModule,
    TuiButton,
    TuiButtonLoading,
    TuiSwitch,
    TuiCardLarge,
    TuiLoader,
    WifiTableComponent,
    TitleDirective,
    RouterLink,
    PlaceholderComponent,
    i18nPipe,
    DocsLinkDirective,
  ],
})
export default class SystemWifiComponent {
  private readonly tasks = inject(TaskService)
  private readonly errorService = inject(ErrorService)
  private readonly api = inject(ApiService)
  private readonly alerts = inject(TuiNotificationService)
  private readonly update$ = new Subject<WifiData>()
  private readonly refresh$ = new Subject<void>()
  private readonly formDialog = inject(FormDialogService)
  private readonly cdr = inject(ChangeDetectorRef)
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly i18n = inject(i18nPipe)

  private readonly status$ = this.patch.watch$('serverInfo', 'network', 'wifi')
  readonly status = toSignal(this.status$)
  readonly wifi = toSignal(
    merge(
      this.status$.pipe(
        first(s => !!s?.interface),
        switchMap(() => this.getWifi$()),
      ),
      this.refresh$.pipe(
        tap(() => this.refreshing.set(true)),
        exhaustMap(() =>
          this.getWifi$().pipe(finalize(() => this.refreshing.set(false))),
        ),
      ),
      this.update$,
    ),
  )

  readonly refreshing = signal(false)

  refresh() {
    this.refresh$.next()
  }

  async onToggle(enable: boolean) {
    this.tasks.run(
      async () => {
        await this.api.rpc.wifi.setEnabled({ enabled: enable })
        if (enable) {
          this.update$.next({ known: [], available: [] })
          this.pollForNetworks()
        }
      },
      enable ? 'Enabling' : 'Disabling',
    )
  }

  other(wifi: WifiData) {
    this.formDialog.open<FormContext<WiFiForm>>(FormComponent, {
      label: wifiSpec.name as i18nKey,
      data: {
        spec: wifiSpec.spec,
        buttons: [
          {
            text: this.i18n.transform('Save for later')!,
            handler: async ({ ssid, password }) =>
              this.save(ssid, password, wifi),
          },
          {
            text: this.i18n.transform('Save and connect')!,
            handler: async ({ ssid, password }) =>
              this.saveAndConnect(ssid, password),
          },
        ],
      },
    })
  }

  async saveAndConnect(ssid: string, password?: string): Promise<boolean> {
    return this.tasks.run(async () => {
      if (password) {
        await this.api.rpc.wifi.add({ ssid, password })
        await this.api.rpc.wifi.connect({ ssid })
      } else {
        await this.api.rpc.wifi.connect({ ssid })
      }

      await this.confirmWifi(ssid)
    }, 'Connecting. This could take a while')
  }

  private pollForNetworks(): void {
    timer(0, 500)
      .pipe(
        takeWhile(() => !this.wifi()?.available?.length),
        takeUntil(timer(5000)),
      )
      .subscribe(() => this.refresh$.next())
  }

  private async confirmWifi(ssid: string): Promise<void> {
    const maxAttempts = 5
    let attempts = 0

    while (true) {
      if (attempts > maxAttempts) {
        this.alerts
          .open('Check credentials and try again', {
            label: 'Failed to connect',
            appearance: 'warning',
          })
          .subscribe()
        break
      }

      try {
        const start = new Date().valueOf()
        const newWifi = await this.api.rpc.wifi.get({}, { timeout: 10000 })
        const end = new Date().valueOf()
        if (newWifi.connected === ssid) {
          this.update$.next(parseWifi(newWifi))
          this.alerts
            .open('Connection successful!', { appearance: 'positive' })
            .subscribe()
          break
        } else {
          attempts++
          const diff = end - start
          // depending on the response time, wait a min of 1000 ms, and a max of 4000 ms in between retries.  Both 1000 and 4000 are arbitrary
          await pauseFor(Math.max(1000, 4000 - diff))
        }
      } catch (e) {
        attempts++
        console.warn(e)
      }
    }
  }

  private getWifi$(): Observable<WifiData> {
    return defer(() => this.api.rpc.wifi.get({}, { timeout: 10000 })).pipe(
      map(res => parseWifi(res)),
      catchError((e: any) => {
        this.errorService.handleError(e)
        return of({ known: [], available: [] })
      }),
    )
  }

  private async save(
    ssid: string,
    password: string,
    wifi: WifiData,
  ): Promise<boolean> {
    return this.tasks.run(async () => {
      await this.api.rpc.wifi.add({ ssid, password })
      wifi.known = wifi.known.concat({
        ssid,
        strength: 0,
        security: [],
        connected: false,
      })
      this.cdr.markForCheck()
    }, 'Saving')
  }
}
