import {
  AfterViewInit,
  Component,
  inject,
  signal,
  computed,
} from '@angular/core'
import { DialogService, ErrorService, i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiCell, TuiIcon, TuiLoader, TuiTitle } from '@taiga-ui/core'
import { TuiAvatar } from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { MatrixComponent } from '../components/matrix.component'
import { MokEnrollmentDialog } from '../components/mok-enrollment.dialog'
import { RemoveMediaDialog } from '../components/remove-media.dialog'
import { ApiService } from '../services/api.service'
import { StateService } from '../services/state.service'

@Component({
  template: `
    <canvas matrix></canvas>
    <section tuiCardLarge>
      <header tuiHeader>
        <hgroup tuiTitle>
          <h2 tuiCell="m">
            <tui-icon icon="@tui.circle-check-big" class="g-positive" />
            {{ 'Setup Complete!' | i18n }}
          </h2>
          @if (!stateService.kiosk) {
            <p tuiSubtitle>
              {{
                'http://start.local was for setup only. It will no longer work.'
                  | i18n
              }}
            </p>
          }
        </hgroup>
      </header>

      @if (!result()) {
        <tui-loader />
      } @else {
        <!-- Step: Restart flow -->
        @if (result()?.needsRestart) {
          <button tuiCell="l" (click)="removeMedia()">
            <span tuiAvatar="@tui.usb" appearance="secondary"></span>
            <span tuiTitle>
              <b>{{ 'Remove Installation Media' | i18n }}</b>
              <span tuiSubtitle>
                {{
                  'Remove USB stick or other installation media from your server'
                    | i18n
                }}
              </span>
            </span>
            @if (usbRemoved()) {
              <tui-icon icon="@tui.circle-check" class="g-positive" />
            }
          </button>

          <!-- Step: Secure Boot Enrollment (when MOK enrolled) -->
          @if (stateService.mokEnrolled) {
            <button
              tuiCell="l"
              [class.disabled]="!usbRemoved()"
              [disabled]="!usbRemoved()"
              (click)="acknowledgeMok()"
            >
              <span tuiAvatar="@tui.shield-check" appearance="secondary"></span>
              <span tuiTitle>
                <b>{{ 'Secure Boot Enrollment' | i18n }}</b>
                <span tuiSubtitle>
                  {{
                    'Prepare for Secure Boot key enrollment on the next reboot'
                      | i18n
                  }}
                </span>
              </span>
              @if (mokAcknowledged()) {
                <tui-icon icon="@tui.circle-check" class="g-positive" />
              }
            </button>
          }

          <!-- Step: Restart Server -->
          <button
            tuiCell="l"
            [class.disabled]="
              !usbRemoved() || (stateService.mokEnrolled && !mokAcknowledged())
            "
            [disabled]="
              !usbRemoved() || (stateService.mokEnrolled && !mokAcknowledged())
            "
            (click)="reboot()"
          >
            <span tuiAvatar="@tui.rotate-cw" appearance="secondary"></span>
            <span tuiTitle>
              <b>{{ 'Restart Server' | i18n }}</b>
              <span tuiSubtitle>
                @if (rebooting()) {
                  {{ 'Waiting for server to come back online' | i18n }}
                } @else if (rebooted()) {
                  {{ 'Server is back online' | i18n }}
                } @else {
                  {{ 'Restart your server to complete setup' | i18n }}
                }
              </span>
            </span>
            @if (rebooting()) {
              <tui-loader />
            } @else if (rebooted()) {
              <tui-icon icon="@tui.circle-check" class="g-positive" />
            }
          </button>
        } @else if (stateService.kiosk) {
          <button tuiCell="l" (click)="exitKiosk()">
            <span tuiAvatar="@tui.log-in" appearance="secondary"></span>
            <span tuiTitle>
              <b>{{ 'Continue to Login' | i18n }}</b>
              <span tuiSubtitle>
                {{ 'Proceed to the StartOS login screen' | i18n }}
              </span>
            </span>
          </button>
        }

        <!-- Step: Open Local Address (non-kiosk only) -->
        @if (!stateService.kiosk) {
          <button
            tuiCell="l"
            [class.disabled]="!canOpenAddress()"
            [disabled]="!canOpenAddress()"
            (click)="openLocalAddress()"
          >
            <span tuiAvatar="@tui.external-link" appearance="secondary"></span>
            <span tuiTitle>
              <b>{{ 'Open Local Address' | i18n }}</b>
              <span tuiSubtitle>{{ lanAddress() }}</span>
            </span>
          </button>
        }
      }
    </section>
  `,
  styles: `
    [tuiCell].disabled {
      opacity: var(--tui-disabled-opacity);
      pointer-events: none;
    }
  `,
  imports: [
    TuiCardLarge,
    TuiCell,
    TuiIcon,
    TuiLoader,
    TuiAvatar,
    MatrixComponent,
    TuiHeader,
    TuiTitle,
    i18nPipe,
  ],
})
export default class SuccessPage implements AfterViewInit {
  private readonly errorService = inject(ErrorService)
  private readonly api = inject(ApiService)
  private readonly dialogs = inject(DialogService)
  private readonly i18n = inject(i18nPipe)

  readonly stateService = inject(StateService)

  readonly result = signal<T.SetupResult | undefined>(undefined)
  readonly lanAddress = signal('')
  readonly usbRemoved = signal(false)
  readonly mokAcknowledged = signal(false)
  readonly rebooting = signal(false)
  readonly rebooted = signal(false)

  readonly canOpenAddress = computed(() => {
    if (this.result()?.needsRestart && !this.rebooted()) return false
    return true
  })

  ngAfterViewInit() {
    setTimeout(() => this.complete(), 500)
  }

  removeMedia() {
    this.dialogs
      .openComponent<boolean>(new PolymorpheusComponent(RemoveMediaDialog), {
        dismissible: false,
        closable: false,
      })
      .subscribe(() => {
        this.usbRemoved.set(true)
      })
  }

  acknowledgeMok() {
    this.dialogs
      .openComponent<boolean>(new PolymorpheusComponent(MokEnrollmentDialog), {
        label: 'Secure Boot',
        size: 'm',
        dismissible: false,
        closable: false,
      })
      .subscribe(() => {
        this.mokAcknowledged.set(true)
      })
  }

  exitKiosk() {
    this.api.rpc.setup.exit({})
  }

  openLocalAddress() {
    window.open(this.lanAddress(), '_blank')
  }

  async reboot() {
    if (this.rebooting() || this.rebooted()) return

    this.rebooting.set(true)

    try {
      await this.api.rpc.setup.exit({})
      await this.pollForServer()
      this.rebooted.set(true)
      this.rebooting.set(false)
    } catch (e: any) {
      this.errorService.handleError(e)
      this.rebooting.set(false)
    }
  }

  private async complete() {
    try {
      const result = await this.api.rpc.setup.complete({})
      this.result.set(result)

      if (!this.stateService.kiosk) {
        this.lanAddress.set(`http://${result.hostname}.local`)

        if (!result.needsRestart) {
          await this.api.rpc.setup.exit({})
        }
      }
    } catch (e: any) {
      this.errorService.handleError(e)
    }
  }

  private async pollForServer(): Promise<void> {
    const maxAttempts = 60
    let attempts = 0

    while (attempts < maxAttempts) {
      try {
        await this.api.echo({ message: 'ping' }, `${this.lanAddress()}/rpc/v1`)
        return
      } catch {
        await new Promise(resolve => setTimeout(resolve, 5000))
        attempts++
      }
    }

    throw new Error(
      this.i18n.transform(
        'Server did not come back online. Please check your server and try accessing it manually.',
      ),
    )
  }
}
