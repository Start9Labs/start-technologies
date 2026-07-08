import { Component, computed, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import { Router } from '@angular/router'
import { WA_IS_MOBILE } from '@ng-web-apis/platform'
import {
  ErrorService,
  i18nService,
  Language,
  LANGUAGES,
  TaskService,
} from '@start9labs/shared'
import { T, utils } from '@start9labs/start-core'
import { TuiResponsiveDialogService } from '@taiga-ui/addon-mobile'
import {
  TuiButton,
  TuiCell,
  TuiIcon,
  TuiInput,
  TuiLoader,
  TuiTitle,
} from '@taiga-ui/core'
import {
  TuiBadge,
  TuiButtonLoading,
  TuiChevron,
  TuiDataListWrapper,
  TuiSelect,
  TuiSwitch,
} from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { PatchDB } from 'patch-db-client'
import { map } from 'rxjs'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { ApiService } from 'src/app/services/api/api.service'
import { AuthService } from 'src/app/services/auth.service'
import { TunnelData } from 'src/app/services/patch-db/data-model'
import { UpdateService } from 'src/app/services/update.service'
import { CHANGE_PASSWORD } from './change-password'

@Component({
  template: `
    <div tuiCardLarge="compact" appearance="floating">
      <div tuiCell>
        <span tuiTitle>
          <strong>
            {{ 'Version' | i18n }}
            @if (update.hasUpdate()) {
              <span tuiBadge appearance="positive" size="s">
                {{ 'Update Available' | i18n }}
              </span>
            }
          </strong>
          <span tuiSubtitle>
            {{ 'Current' | i18n }}: {{ update.installed() ?? '—' }}
          </span>
        </span>
        @if (update.hasUpdate()) {
          <button tuiButton size="s" [loading]="applying()" (click)="onApply()">
            {{ 'Update to' | i18n }} {{ update.candidate() }}
          </button>
        } @else {
          <button
            tuiButton
            size="s"
            appearance="secondary"
            [loading]="checking()"
            (click)="onCheckUpdate()"
          >
            {{ 'Check for updates' | i18n }}
          </button>
        }
      </div>
    </div>
    <div tuiCardLarge="compact" appearance="floating">
      <header tuiHeader="body-l">
        <tui-icon icon="@tui.milestone" />
        <h3 tuiTitle>{{ 'HTTP Redirect (80 → 443)' | i18n }}</h3>
      </header>
      <table class="g-table no-actions">
        <thead>
          <tr>
            <th>{{ 'WAN IP' | i18n }}</th>
            <th>{{ 'Enabled' | i18n }}</th>
          </tr>
        </thead>
        <tbody>
          @for (redirect of redirects(); track redirect.ip) {
            <tr>
              <td>{{ redirect.ip }}</td>
              <td>
                <tui-loader
                  size="xs"
                  [loading]="toggling() === redirect.ip"
                  [overlay]="true"
                >
                  <input
                    tuiSwitch
                    type="checkbox"
                    size="s"
                    [style.display]="'flex'"
                    [showIcons]="false"
                    [disabled]="redirect.forwarded"
                    [ngModel]="redirect.enabled && !redirect.forwarded"
                    (ngModelChange)="onToggleRedirect(redirect)"
                  />
                </tui-loader>
              </td>
            </tr>
          } @empty {
            <tr>
              <td colspan="2">{{ 'No public IPv4 addresses' | i18n }}</td>
            </tr>
          }
        </tbody>
      </table>
    </div>
    <div tuiCardLarge="compact" appearance="floating">
      <tui-textfield
        tuiChevron
        [tuiTextfieldCleaner]="false"
        [identityMatcher]="matchLanguage"
        [stringify]="stringifyLanguage"
      >
        <label tuiLabel>{{ 'Language' | i18n }}</label>
        @if (mobile) {
          <select
            tuiSelect
            [ngModel]="language()"
            [items]="languages"
            (ngModelChange)="setLanguage($event)"
          ></select>
        } @else {
          <input
            tuiSelect
            [ngModel]="language()"
            (ngModelChange)="setLanguage($event)"
          />
        }
        @if (!mobile) {
          <tui-data-list-wrapper *tuiDropdown [items]="languages" />
        }
      </tui-textfield>
    </div>
    <div
      tuiCardLarge="compact"
      appearance="floating"
      [style.align-items]="'start'"
    >
      <button tuiButton size="s" (click)="onChangePassword()">
        {{ 'Change password' | i18n }}
      </button>
      <button
        tuiButton
        size="s"
        iconStart="@tui.rotate-cw"
        [loading]="restarting()"
        (click)="onRestart()"
      >
        {{ 'Reboot VPS' | i18n }}
      </button>
      <button tuiButton size="s" iconStart="@tui.log-out" (click)="onLogout()">
        {{ 'Logout' | i18n }}
      </button>
    </div>
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      gap: 1rem;
      max-inline-size: 32rem;
    }

    // The table keeps its full width so the WAN IP column has room; only the
    // Enabled column shrinks to its toggle, instead of the two splitting evenly
    // and leaving the toggle floating in a half-width column.
    .g-table th:last-child,
    .g-table td:last-child {
      width: 1%;
      white-space: nowrap;
    }
  `,
  imports: [
    FormsModule,
    TuiCardLarge,
    TuiCell,
    TuiTitle,
    TuiHeader,
    TuiIcon,
    TuiInput,
    TuiButton,
    TuiButtonLoading,
    TuiBadge,
    TuiChevron,
    TuiDataListWrapper,
    TuiLoader,
    TuiSelect,
    TuiSwitch,
    i18nPipe,
  ],
})
export default class Settings {
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly errorService = inject(ErrorService)
  private readonly api = inject(ApiService)
  private readonly auth = inject(AuthService)
  private readonly router = inject(Router)
  private readonly tasks = inject(TaskService)
  private readonly patch = inject<PatchDB<TunnelData>>(PatchDB)
  private readonly i18n = inject(i18nPipe)
  private readonly i18nSvc = inject(i18nService)

  protected readonly update = inject(UpdateService)
  protected readonly checking = signal(false)
  protected readonly applying = signal(false)
  protected readonly restarting = signal(false)
  protected readonly toggling = signal<string | null>(null)

  protected readonly mobile = inject(WA_IS_MOBILE)
  protected readonly languages = LANGUAGES
  protected readonly language = signal(
    LANGUAGES.find(l => l.name === this.i18nSvc.lang) ?? LANGUAGES[0]!,
  )
  protected readonly stringifyLanguage = (l: Language) => l.nativeName
  protected readonly matchLanguage = (a: Language, b: Language) =>
    a.name === b.name

  protected setLanguage(language: Language): void {
    this.language.set(language)
    this.i18nSvc.setLang(language.name)
  }

  private readonly ips = toSignal(
    this.patch.watch$('gateways').pipe(
      map(g =>
        Object.values(g)
          .flatMap(
            val => val.ipInfo?.subnets.map(s => utils.IpNet.parse(s)) || [],
          )
          .filter(s => s.isIpv4() && s.isPublic())
          .map(s => s.address),
      ),
    ),
    { initialValue: [] },
  )
  private readonly httpRedirects = toSignal(this.patch.watch$('httpRedirects'))
  private readonly portForwards = toSignal(this.patch.watch$('portForwards'))
  protected readonly redirects = computed(() => {
    const disabled = new Set(this.httpRedirects()?.disabled || [])
    const forwards = this.portForwards() || {}
    return this.ips().map(ip => ({
      ip,
      enabled: !disabled.has(ip),
      forwarded: portEightyForwarded(forwards, ip),
    }))
  })

  protected async onToggleRedirect(redirect: {
    ip: string
    enabled: boolean
    forwarded: boolean
  }) {
    if (redirect.forwarded) return

    this.toggling.set(redirect.ip)

    try {
      await this.api.setHttpRedirectEnabled({
        ip: redirect.ip,
        enabled: !redirect.enabled,
      })
    } catch (e: any) {
      this.errorService.handleError(e)
    } finally {
      this.toggling.set(null)
    }
  }

  protected onChangePassword(): void {
    this.dialogs
      .open(CHANGE_PASSWORD, { label: this.i18n.transform('Change Password') })
      .subscribe()
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
          this.i18n.transform(
            'The VPS is rebooting. Please wait 1\u20132 minutes, then refresh the page.',
          ),
          {
            label: this.i18n.transform('Rebooting'),
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
    this.tasks.run(async () => {
      await this.api.logout()
      this.auth.authenticated.set(false)
      this.router.navigate(['/'])
    })
  }
}

// Whether any forward on `ip` covers port 80 (a DNAT range or an SNI/single
// entry) — the redirect and a port-80 forward are mutually exclusive.
function portEightyForwarded(
  forwards: Record<string, T.Tunnel.PortForward>,
  ip: string,
): boolean {
  return Object.entries(forwards).some(([source, pf]) => {
    const idx = source.lastIndexOf(':')
    if (idx < 0 || source.slice(0, idx) !== ip) return false
    const start = Number(source.slice(idx + 1))
    const span = pf.kind === 'dnat' ? (pf.count ?? 1) : 1
    return start <= 80 && 80 <= start + span - 1
  })
}
