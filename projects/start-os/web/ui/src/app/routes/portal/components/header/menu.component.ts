import { Component, inject } from '@angular/core'
import { RouterLink } from '@angular/router'
import {
  DialogService,
  DocsLinkDirective,
  i18nPipe,
  SafeLinksDirective,
  TaskService,
} from '@start9labs/shared'
import {
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiHint,
  TuiIcon,
} from '@taiga-ui/core'
import { filter } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { AuthService } from 'src/app/services/auth.service'
import { STATUS } from 'src/app/services/status.service'
import { ABOUT } from './about.component'

@Component({
  selector: 'header-menu',
  template: `
    <button
      tuiIconButton
      appearance=""
      tuiHintDirection="bottom"
      [tuiHint]="open ? '' : ('Start Menu' | i18n)"
      [tuiHintShowDelay]="750"
      [tuiDropdown]="content"
      [(tuiDropdownOpen)]="open"
      [tuiDropdownMaxHeight]="9999"
    >
      <img [style.max-width.%]="60" src="assets/img/icon.png" alt="StartOS" />
    </button>
    <ng-template #content>
      @if (status().status !== 'success') {
        <div class="status">
          <tui-icon [icon]="status().icon" />
          {{ status().message | i18n }}
        </div>
      }
      <tui-data-list safeLinks [style.width.rem]="13">
        <button tuiOption iconStart="@tui.info" (click)="about()">
          {{ 'About this server' | i18n }}
        </button>
        <hr />
        <a
          tuiOption
          docsLink
          iconStart="@tui.book-open-text"
          path="/start-os/index.html"
        >
          {{ 'User manual' | i18n }}
        </a>
        <a
          tuiOption
          iconStart="@tui.headphones"
          href="https://start9.com/contact"
        >
          {{ 'Contact support' | i18n }}
        </a>
        <a
          tuiOption
          iconStart="@tui.dollar-sign"
          href="https://donate.start9.com"
        >
          {{ 'Donate to Start9' | i18n }}
        </a>
        <hr />
        <a
          tuiOption
          iconStart="@tui.settings"
          routerLink="/system"
          (click)="open = false"
        >
          {{ 'System Settings' | i18n }}
        </a>
        <hr />
        <button
          tuiOption
          iconStart="@tui.refresh-cw"
          (click)="promptPower('restart')"
        >
          {{ 'Restart' | i18n }}
        </button>
        <button
          tuiOption
          iconStart="@tui.power"
          (click)="promptPower('shutdown')"
        >
          {{ 'Shutdown' | i18n }}
        </button>
        <button tuiOption iconStart="@tui.log-out" (click)="logout()">
          {{ 'Logout' | i18n }}
        </button>
      </tui-data-list>
    </ng-template>
  `,
  styles: `
    :host {
      padding-inline-start: 0.5rem;

      &._open::before {
        filter: brightness(1.2);
      }
    }

    .status {
      display: flex;
      gap: 1rem;
      align-items: center;
      padding: 1rem 1rem 0.5rem;
      opacity: 0.5;
    }

    :host-context(tui-root._mobile) {
      [tuiIconButton] {
        box-shadow: inset -1.25rem 0 0 -1rem var(--status);
      }
    }
  `,
  host: { '[class._open]': 'open' },
  imports: [
    TuiDropdown,
    TuiDataList,
    TuiButton,
    TuiIcon,
    RouterLink,
    i18nPipe,
    TuiHint,
    DocsLinkDirective,
    SafeLinksDirective,
  ],
})
export class HeaderMenuComponent {
  private readonly api = inject(ApiService)
  private readonly auth = inject(AuthService)
  private readonly tasks = inject(TaskService)
  private readonly dialog = inject(DialogService)

  open = false

  readonly status = inject(STATUS)

  about() {
    this.dialog.openComponent(ABOUT, { label: 'About this server' }).subscribe()
  }

  async promptPower(action: 'restart' | 'shutdown') {
    this.dialog
      .openConfirm(
        action === 'restart'
          ? {
              label: 'Restart',
              size: 's',
              data: {
                content:
                  'Are you sure you want to restart your server? It can take several minutes to come back online.',
                yes: 'Restart',
                no: 'Cancel',
              },
            }
          : {
              label: 'Warning',
              size: 's',
              data: {
                content:
                  'Are you sure you want to power down your server? This can take several minutes, and your server will not come back online automatically. To power on again, You will need to physically unplug your server and plug it back in.',
                yes: 'Shutdown',
                no: 'Cancel',
              },
            },
      )
      .pipe(filter(Boolean))
      .subscribe(() =>
        this.tasks.run(
          async () =>
            await this.api[
              action === 'restart' ? 'restartServer' : 'shutdownServer'
            ]({}),
          `Beginning ${action}`,
        ),
      )
  }

  logout() {
    this.api.logout({}).catch(e => console.error('Failed to log out', e))
    this.auth.logout()
  }
}
