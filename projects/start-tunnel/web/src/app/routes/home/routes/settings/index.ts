import { Component, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import {
  NonNullableFormBuilder,
  ReactiveFormsModule,
  ValidatorFn,
  Validators,
} from '@angular/forms'
import { Router } from '@angular/router'
import { ErrorService } from '@start9labs/shared'
import { utils } from '@start9labs/start-core'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import {
  TuiButton,
  TuiCell,
  TuiError,
  TuiInput,
  TuiNotificationService,
  TuiTextfield,
  TuiTitle,
  tuiValidationErrorsProvider,
} from '@taiga-ui/core'
import {
  TuiBadge,
  TuiButtonLoading,
  TuiNotificationMiddleService,
} from '@taiga-ui/kit'
import { TuiCardLarge, TuiForm } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'
import { map } from 'rxjs'
import { ApiService } from 'src/app/services/api/api.service'
import { AuthService } from 'src/app/services/auth.service'
import { TunnelData } from 'src/app/services/patch-db/data-model'
import { UpdateService } from 'src/app/services/update.service'
import { CHANGE_PASSWORD } from './change-password'

// Empty is allowed (clears the prefix); otherwise the value must be an IPv6
// address with an explicit /prefix in [0, 128]. utils.IpNet.parse does not
// enforce the prefix length, so validate the `/len` ourselves. The daemon
// revalidates, so this is just UX.
const ipv6Prefix: ValidatorFn = ({ value }) => {
  if (!value) return null
  const parts = value.split('/')
  const len = Number(parts[1])
  if (parts.length !== 2 || !Number.isInteger(len) || len < 0 || len > 128) {
    return { ipv6: true }
  }
  try {
    if (utils.IpAddress.parse(parts[0]).isIpv4()) return { ipv6: true }
    return null
  } catch {
    return { ipv6: true }
  }
}

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
      <span tuiTitle>
        <strong>IPv6</strong>
        <span tuiSubtitle>
          Delegate global IPv6 addresses to devices from a prefix your VPS
          routes to this server. Currently: {{ ipv6() ?? 'not configured' }}
        </span>
      </span>
      <form tuiForm="m" [formGroup]="form">
        <tui-textfield>
          <label tuiLabel>Routed prefix</label>
          <input
            tuiInput
            placeholder="2001:db8:abcd::/64"
            formControlName="prefix"
          />
        </tui-textfield>
        <tui-error formControlName="prefix" />
      </form>
      <div [style.display]="'flex'" [style.gap.rem]="0.5">
        <button
          tuiButton
          size="s"
          [loading]="savingIpv6()"
          [disabled]="form.invalid"
          (click)="onSaveIpv6()"
        >
          Save
        </button>
        @if (ipv6()) {
          <button
            tuiButton
            size="s"
            appearance="secondary"
            [loading]="savingIpv6()"
            (click)="onClearIpv6()"
          >
            Disable
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
  providers: [
    tuiValidationErrorsProvider({ ipv6: 'Enter a valid IPv6 prefix' }),
  ],
  imports: [
    ReactiveFormsModule,
    TuiCardLarge,
    TuiCell,
    TuiTitle,
    TuiButton,
    TuiButtonLoading,
    TuiBadge,
    TuiError,
    TuiForm,
    TuiInput,
    TuiTextfield,
  ],
})
export default class Settings {
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly errorService = inject(ErrorService)
  private readonly api = inject(ApiService)
  private readonly auth = inject(AuthService)
  private readonly router = inject(Router)
  private readonly loading = inject(TuiNotificationMiddleService)
  private readonly alerts = inject(TuiNotificationService)
  private readonly patch = inject<PatchDB<TunnelData>>(PatchDB)

  protected readonly update = inject(UpdateService)
  protected readonly checking = signal(false)
  protected readonly applying = signal(false)
  protected readonly restarting = signal(false)
  protected readonly savingIpv6 = signal(false)

  protected readonly ipv6 = toSignal(
    this.patch.watch$('wg', 'ipv6').pipe(map(v => v ?? null)),
    { initialValue: null },
  )

  protected readonly form = inject(NonNullableFormBuilder).group({
    prefix: ['', [Validators.maxLength(49), ipv6Prefix]],
  })

  protected onChangePassword(): void {
    this.dialogs.open(CHANGE_PASSWORD, { label: 'Change Password' }).subscribe()
  }

  protected async onSaveIpv6() {
    await this.setIpv6(this.form.getRawValue().prefix || null)
  }

  protected async onClearIpv6() {
    await this.setIpv6(null)
    this.form.reset()
  }

  private async setIpv6(prefix: string | null) {
    this.savingIpv6.set(true)

    try {
      await this.api.setIpv6({ prefix })
      this.alerts
        .open(prefix ? 'IPv6 prefix set' : 'IPv6 disabled', {
          label: 'Success',
          appearance: 'positive',
        })
        .subscribe()
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      this.savingIpv6.set(false)
    }
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
          'The VPS is rebooting. Please wait 1–2 minutes, then refresh the page.',
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
