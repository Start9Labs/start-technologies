import { Component, inject, signal } from '@angular/core'
import { Router } from '@angular/router'
import { ErrorService } from '@start9labs/shared'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import { TuiButton, TuiCell, TuiTitle } from '@taiga-ui/core'
import {
  TuiBadge,
  TuiButtonLoading,
  TuiNotificationMiddleService,
} from '@taiga-ui/kit'
import { TuiCardLarge } from '@taiga-ui/layout'
import { ApiService } from 'src/app/services/api/api.service'
import { AuthService } from 'src/app/services/auth.service'
import { UpdateService } from 'src/app/services/update.service'
import { CHANGE_PASSWORD } from './change-password'

@Component({
  template: `
    <div tuiCardLarge="compact" appearance="floating">
      <div tuiCell>
        <span tuiTitle>
          <strong>
            Version
            @if (update.hasUpdate()) {
              <span tuiBadge appearance="positive" size="s">
                Update Available
              </span>
            }
          </strong>
          <span tuiSubtitle>Current: {{ update.installed() ?? '—' }}</span>
        </span>
        @if (update.hasUpdate()) {
          <button tuiButton size="s" [loading]="applying()" (click)="onApply()">
            Update to {{ update.candidate() }}
          </button>
        } @else {
          <button
            tuiButton
            size="s"
            appearance="secondary"
            [loading]="checking()"
            (click)="onCheckUpdate()"
          >
            Check for updates
          </button>
        }
      </div>
    </div>
    <div
      tuiCardLarge="compact"
      appearance="floating"
      [style.align-items]="'start'"
    >
      <button tuiButton size="s" (click)="onChangePassword()">
        Change password
      </button>
      <button
        tuiButton
        size="s"
        iconStart="@tui.rotate-cw"
        [loading]="restarting()"
        (click)="onRestart()"
      >
        Reboot VPS
      </button>
      <button tuiButton size="s" iconStart="@tui.log-out" (click)="onLogout()">
        Logout
      </button>
    </div>
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      gap: 1rem;
      max-inline-size: 50rem;
    }
  `,
  imports: [
    TuiCardLarge,
    TuiCell,
    TuiTitle,
    TuiButton,
    TuiButtonLoading,
    TuiBadge,
  ],
})
export default class Settings {
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly errorService = inject(ErrorService)
  private readonly api = inject(ApiService)
  private readonly auth = inject(AuthService)
  private readonly router = inject(Router)
  private readonly loading = inject(TuiNotificationMiddleService)

  protected readonly update = inject(UpdateService)
  protected readonly checking = signal(false)
  protected readonly applying = signal(false)
  protected readonly restarting = signal(false)

  protected onChangePassword(): void {
    this.dialogs.open(CHANGE_PASSWORD, { label: 'Change Password' }).subscribe()
  }

  protected async onCheckUpdate() {
    this.checking.set(true)

    try {
      await this.update.checkUpdate()
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      this.checking.set(false)
    }
  }

  protected async onApply() {
    this.applying.set(true)

    try {
      await this.update.applyUpdate()
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      this.applying.set(false)
    }
  }

  protected async onRestart() {
    this.restarting.set(true)

    try {
      await this.api.restart()
      this.dialogs
        .open(
          'The VPS is rebooting. Please wait 1\u20132 minutes, then refresh the page.',
          {
            label: 'Rebooting',
          },
        )
        .subscribe()
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      this.restarting.set(false)
    }
  }

  protected async onLogout() {
    const loader = this.loading.open('').subscribe()

    try {
      await this.api.logout()
      this.auth.authenticated.set(false)
      this.router.navigate(['/'])
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      loader.unsubscribe()
    }
  }
}
