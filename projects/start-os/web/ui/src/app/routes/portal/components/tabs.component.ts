import {
  Component,
  computed,
  inject,
  signal,
  TemplateRef,
  viewChildren,
} from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { RouterLink, RouterLinkActive } from '@angular/router'
import { i18nPipe } from '@start9labs/shared'
import { TuiResponsiveDialogService, TuiTabBar } from '@taiga-ui/addon-mobile'
import { TuiIcon } from '@taiga-ui/core'
import { TuiBadgeNotification } from '@taiga-ui/kit'
import { BadgeService } from 'src/app/services/badge.service'
import { getMenu } from 'src/app/utils/system-utilities'
import { UpdatesCount } from './updates-count'

const FILTER = ['services', 'system', 'marketplace']

@Component({
  selector: 'app-tabs',
  template: `
    <nav tuiTabBar [(activeItemIndex)]="index">
      <a
        tuiTabBarItem
        icon="@tui.layout-grid"
        routerLink="/services"
        routerLinkActive
        (isActiveChange)="update()"
      >
        {{ 'Services' | i18n }}
      </a>
      <a
        tuiTabBarItem
        icon="@tui.shopping-cart"
        routerLink="/marketplace"
        routerLinkActive
        (isActiveChange)="update()"
      >
        {{ 'Marketplace' | i18n }}
      </a>
      <a
        tuiTabBarItem
        icon="@tui.settings"
        routerLink="/system"
        routerLinkActive
        [badge]="badge()"
        (isActiveChange)="update()"
      >
        {{ 'System' | i18n }}
      </a>
      <button
        tuiTabBarItem
        icon="@tui.ellipsis"
        (click)="more(content)"
        [badge]="all()"
      >
        {{ 'More' | i18n }}
        <ng-template #content let-observer>
          @for (item of menu; track $index) {
            <a
              class="item"
              routerLinkActive="item_active"
              [routerLink]="['/', item.routerLink]"
              (click)="observer.complete()"
            >
              <tui-icon [icon]="item.icon" />
              {{ item.name }}
              @if (item.badge(); as badge) {
                <tui-badge-notification
                  [appUpdatesCount]="item.routerLink === 'updates'"
                >
                  {{ badge }}
                </tui-badge-notification>
              } @else if (item.warning()) {
                <tui-icon
                  icon="@tui.triangle-alert"
                  class="g-warning"
                  role="img"
                  [attr.aria-label]="'Warning' | i18n"
                  [attr.aria-hidden]="false"
                />
              }
            </a>
          }
        </ng-template>
      </button>
    </nav>
  `,
  styles: `
    @use '@taiga-ui/styles/utils' as taiga;

    :host {
      display: none;
      backdrop-filter: blur(1rem);
      --tui-background-elevation-1: #333;
      --tui-background-base: #fff;
      --tui-border-normal: var(--tui-background-neutral-1);
      --tui-status-negative: #f52222;
    }

    [tuiTabBar]::before {
      opacity: 0.7;
    }

    .item {
      @include taiga.button-clear();

      display: flex;
      padding: 0.75rem 0.25rem;
      gap: 1rem;
      align-items: center;

      &_active {
        color: var(--tui-text-action);
      }
    }

    :host-context(tui-root._mobile) {
      display: block;
    }
  `,
  imports: [
    RouterLink,
    RouterLinkActive,
    TuiTabBar,
    TuiBadgeNotification,
    UpdatesCount,
    TuiIcon,
    i18nPipe,
  ],
})
export class TabsComponent {
  private readonly dialogs = inject(TuiResponsiveDialogService)
  private readonly links = viewChildren(RouterLinkActive)

  readonly index = signal(3)

  readonly menu = getMenu().filter(item => !FILTER.includes(item.routerLink))
  readonly badge = toSignal(inject(BadgeService).getCount('system'), {
    initialValue: 0,
  })

  readonly all = computed(() =>
    this.menu.reduce((acc, item) => acc + item.badge(), 0),
  )

  more(content: TemplateRef<any>) {
    this.dialogs.open(content, { label: 'StartOS' }).subscribe({
      complete: () => this.update(),
    })
  }

  update() {
    const index = this.links().findIndex(link => link.isActive)
    this.index.set(index === -1 ? 3 : index)
  }
}
