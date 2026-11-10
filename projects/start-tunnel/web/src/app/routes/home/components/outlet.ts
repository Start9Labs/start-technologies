import { Component, inject, linkedSignal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import {
  NavigationEnd,
  Router,
  RouterLink,
  RouterOutlet,
} from '@angular/router'
import { TUI_BREAKPOINT, TuiIcon, TuiScrollbar } from '@taiga-ui/core'
import { TuiBlock, TuiSwitch } from '@taiga-ui/kit'
import { TuiNavigation } from '@taiga-ui/layout'
import { filter } from 'rxjs'
import { Aside } from 'src/app/help/aside'
import { HELP_OPEN } from 'src/app/help/help'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { UpdateService } from 'src/app/services/update.service'

@Component({
  selector: 'app-outlet',
  template: `
    <header tuiNavigationHeader>
      <tui-icon icon="assets/icons/favicon.svg" />
      <h1>StartTunnel</h1>
      <label class="help-toggle" tuiBlock="s" appearance="secondary-grayscale">
        <input type="checkbox" tuiSwitch size="s" [(ngModel)]="help" />
        {{ 'Help' | i18n }}
      </label>
    </header>
    <section>
      <aside [tuiNavigationAside]="open()">
        @for (route of routes; track $index) {
          <a
            tuiAsideItem
            tuiHintAppearance="primary-grayscale"
            [iconStart]="route.icon"
            [routerLink]="route.link"
          >
            {{ route.name | i18n }}
          </a>
        }
        <a
          tuiAsideItem
          tuiHintAppearance="primary-grayscale"
          iconStart="@tui.settings"
          routerLink="settings"
          [iconEnd]="update.hasUpdate() ? '@tui.rocket' : ''"
        >
          {{ 'Settings' | i18n }}
        </a>
        <footer>
          <button
            tuiAsideItem
            tuiHintAppearance="primary-grayscale"
            type="button"
            [iconStart]="open() ? '@tui.chevron-left' : '@tui.chevron-right'"
            (click)="open.set(!open())"
          >
            {{ (open() ? 'Collapse' : 'Expand') | i18n }}
          </button>
        </footer>
      </aside>
      <main tuiNavigationMain>
        <tui-scrollbar>
          <router-outlet />
        </tui-scrollbar>
      </main>
      <aside appAside></aside>
    </section>
  `,
  styles: `
    :host-context(tui-root._mobile) {
      main {
        min-inline-size: calc(100vw - 3rem);
        border: 0.375rem solid transparent;
      }

      tui-scrollbar {
        border-start-start-radius: 1rem;
        block-size: 100%;
        padding: 0;

        // Mobile drops the 1.5rem end padding: the thumb's painted 0.25rem, inside a 0.25rem transparent border, moves into main's 0.375rem border.
        > ::ng-deep tui-scroll-controls {
          transform: translateX(0.625rem);
        }
      }
    }

    :host {
      display: flex;
      flex-direction: column;
      block-size: 100%;
      overflow: hidden;

      --tui-theme-color: var(--tui-background-elevation-1);

      header {
        clip-path: inset(0);
      }

      h1 {
        font: var(--tui-typography-body-l);
        font-weight: bold;
      }

      tui-icon {
        margin-inline: 0.25rem 0.5rem;
      }

      .help-toggle {
        margin-inline: auto 0.5rem;
        border-radius: 2rem;
      }

      section {
        display: flex;
        flex: 1;
        overflow: hidden;
        background: var(--tui-background-base-alt);
      }

      // Less its top padding, so it ends at main's 1.5rem bottom border and that border is the gap below the content.
      tui-scrollbar {
        block-size: calc(100% - 1.5rem);
        min-inline-size: 100%;
        padding-block-start: 1.5rem;
        padding-inline-end: 1.5rem;
        border-radius: var(--tui-radius-s);

        > ::ng-deep tui-scroll-controls {
          transform: translateX(1.5rem);
        }
      }

      aside {
        --tui-background-neutral-1-hover: var(--tui-background-elevation-2);
        --tui-background-neutral-1-pressed: var(--tui-background-elevation-3);
      }

      [tuiAsideItem]::after {
        color: var(--tui-status-positive);
      }
    }
  `,
  imports: [
    RouterOutlet,
    TuiNavigation,
    RouterLink,
    TuiIcon,
    TuiScrollbar,
    FormsModule,
    TuiBlock,
    TuiSwitch,
    Aside,
    i18nPipe,
  ],
})
export class Outlet {
  protected readonly router = inject(Router)
  protected readonly breakpoint = inject(TUI_BREAKPOINT)
  protected readonly update = inject(UpdateService)
  protected readonly help = inject(HELP_OPEN)
  protected readonly routes = [
    {
      name: 'Subnets',
      icon: '@tui.network',
      link: 'subnets',
    },
    {
      name: 'Devices',
      icon: '@tui.laptop',
      link: 'devices',
    },
    {
      name: 'Published Ports',
      icon: '@tui.globe',
      link: 'published-ports',
    },
    {
      name: 'DNS',
      icon: '@tui.list',
      link: 'dns',
    },
  ] as const

  protected readonly title = toSignal(
    this.router.events.pipe(filter(event => event instanceof NavigationEnd)),
  )

  protected readonly open = linkedSignal<string[], boolean>({
    source: () => [this.breakpoint(), String(this.title())],
    computation: (source, previous) =>
      previous?.value !== false && source[0] !== 'mobile',
  })
}
