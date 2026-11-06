import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { RouterModule } from '@angular/router'
import { i18nPipe } from '@start9labs/shared'
import { TuiCell, TuiIcon, TuiLoader, TuiTitle } from '@taiga-ui/core'
import { TuiBadgeNotification } from '@taiga-ui/kit'

import { BadgeService } from 'src/app/services/badge.service'
import { OSService } from 'src/app/services/os.service'
import { TitleDirective } from 'src/app/services/title.service'
import { SYSTEM_MENU } from './system.const'

@Component({
  template: `
    <span *title>{{ 'System' | i18n }}</span>
    <aside class="g-aside">
      @for (cat of menu; track $index) {
        @if ($index) {
          <hr />
        }
        @for (page of cat; track $index) {
          <a
            #activeLink="routerLinkActive"
            tuiCell="s"
            routerLinkActive="active"
            [routerLink]="page.link"
          >
            <tui-icon [icon]="page.icon" />
            <span tuiTitle>
              <span>
                {{ page.item | i18n }}
                @if (page.item === 'General Settings' && generalBadge()) {
                  <tui-badge-notification>
                    {{ generalBadge() }}
                  </tui-badge-notification>
                }
                @if (
                  page.item === 'Backups' &&
                  backupsBadge() &&
                  (!activeLink.isActive || !backupProgressActive())
                ) {
                  <tui-loader class="backup-progress-indicator" size="s" />
                }
              </span>
            </span>
          </a>
        }
      }
    </aside>
    <router-outlet />
  `,
  styles: `
    :host {
      display: flex;
      padding: 0;
    }

    tui-badge-notification {
      vertical-align: baseline;
    }

    .backup-progress-indicator {
      display: inline-flex;
      margin-inline-start: 0.35rem;
      color: var(--tui-text-action);
      vertical-align: middle;
    }

    hr {
      height: 1px;
      margin: 0.5rem;
      background: var(--tui-border-normal);
      border: none;
    }

    [tuiCell] {
      color: var(--tui-text-secondary);

      &.active {
        color: var(--tui-text-primary);

        [tuiTitle] {
          font-weight: bold;
        }
      }
    }

    span:not(:last-child) {
      display: none;
    }

    router-outlet + ::ng-deep *:not(.g-subpage) {
      height: fit-content;
      flex: 1;
      display: flex;
      flex-direction: column;
      gap: 1rem;
      padding: 1rem;
    }

    :host-context(tui-root._mobile) {
      padding-inline: 0.75rem;

      aside {
        padding: 0;
        width: 100%;
        background: none;
        box-shadow: none;

        &:not(:nth-last-child(2)) {
          display: none;
        }
      }

      [tuiCell] {
        color: var(--tui-text-primary);
        margin: 0.5rem 0;

        [tuiTitle] {
          font: var(--tui-typography-body-l);
        }
      }

      ::ng-deep hgroup h3 {
        display: none;
      }
    }
  `,
  host: { class: 'g-page' },
  imports: [
    RouterModule,
    TuiCell,
    TuiIcon,
    TuiLoader,
    TuiTitle,
    TitleDirective,
    TuiBadgeNotification,
    i18nPipe,
  ],
})
export class SystemComponent {
  protected readonly menu = SYSTEM_MENU
  protected readonly generalBadge = toSignal(
    inject(BadgeService).getCount('general'),
  )
  protected readonly backupsBadge = toSignal(
    inject(BadgeService).getCount('backups'),
  )
  protected readonly backupProgressActive = toSignal(
    inject(OSService).backingUp$,
    {
      initialValue: false,
    },
  )
}
