import { Component, computed, inject, signal } from '@angular/core'
import { toObservable, toSignal } from '@angular/core/rxjs-interop'
import {
  ActivatedRoute,
  NavigationEnd,
  Router,
  RouterModule,
} from '@angular/router'
import { i18nKey, i18nPipe } from '@start9labs/shared'
import {
  TuiAppearance,
  TuiButton,
  TuiDataList,
  TuiDropdown,
  TuiCell,
  TuiIcon,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiAvatar, TuiFade } from '@taiga-ui/kit'
import { PatchDB } from 'patch-db-client'
import {
  distinctUntilChanged,
  filter,
  map,
  startWith,
  switchMap,
  tap,
} from 'rxjs'
import { PackageIconService } from 'src/app/services/package-icon.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { CornerDirective, TitleDirective } from 'src/app/services/title.service'
import { getCurrentManifest, getManifest } from 'src/app/utils/get-package-data'

type NavItem = { title: i18nKey; icon: string; link: string; exact: boolean }

@Component({
  template: `
    @if (service()) {
      <div
        *title
        class="title"
        tabindex="-1"
        [style.--background]="'url(' + icon() + ')'"
      >
        <a
          tuiIconButton
          iconStart="@tui.arrow-left"
          [routerLink]="current() ? './' : '..'"
        >
          {{ 'Back' | i18n }}
        </a>
        <span
          tuiAvatar
          size="xs"
          [round]="false"
          [style.margin-inline-end.rem]="0.75"
        >
          <img alt="" [src]="icon()" />
        </span>
        @if (current(); as current) {
          <span tuiFade class="section">{{ current.title | i18n }}</span>
        } @else {
          <span tuiFade>{{ manifest()?.title }}</span>
        }
      </div>
      <ng-container *corner>
        <button
          tuiIconButton
          tuiDropdown
          tuiDropdownAlign="end"
          appearance=""
          iconStart="@tui.menu"
          [tuiAppearanceState]="menuOpen() ? 'hover' : null"
          [(tuiDropdownOpen)]="menuOpen"
        >
          {{ 'Menu' | i18n }}
          <tui-data-list *tuiDropdown (click)="menuOpen.set(false)">
            @for (group of nav(); track $index) {
              @if ($index) {
                <hr />
              }
              @for (item of group; track item.link) {
                <a
                  tuiOption
                  routerLinkActive="active"
                  [routerLinkActiveOptions]="{ exact: item.exact }"
                  [iconStart]="item.icon"
                  [routerLink]="item.link"
                >
                  {{ item.title | i18n }}
                </a>
              }
            }
          </tui-data-list>
        </button>
      </ng-container>
      <aside class="g-aside">
        <header tuiCell routerLink="./">
          <span tuiAvatar appearance="action-grayscale" [round]="false">
            <img alt="" [src]="icon()" />
          </span>
          <span tuiTitle>
            <strong tuiFade>{{ manifest()?.title }}</strong>
            <span tuiSubtitle>{{ manifest()?.version }}</span>
          </span>
        </header>
        <nav>
          @for (group of nav(); track $index) {
            @if ($index) {
              <hr />
            }
            @for (item of group; track item.link) {
              <a
                tuiCell="s"
                tuiAppearance="action-grayscale"
                routerLinkActive="active"
                [routerLinkActiveOptions]="{ exact: item.exact }"
                [attr.aria-label]="item.title | i18n"
                [routerLink]="item.link"
              >
                <tui-icon [icon]="item.icon" />
                <span tuiTitle>{{ item.title | i18n }}</span>
              </a>
            }
          }
        </nav>
      </aside>
    }
    <router-outlet />
  `,
  host: {
    class: 'g-page',
    '[style.--background]': '"url(" + icon() + ")"',
  },
  styles: `
    :host {
      display: flex;
      padding: 0;
    }

    .title {
      display: flex;
      align-items: center;
      margin-inline-start: -1rem;

      &:not(:only-child) {
        display: none;
      }
    }

    [tuiSubtitle] {
      text-transform: lowercase;
    }

    header {
      margin: -0.5rem -0.5rem 0.5rem;
      padding-top: 1rem;
      border-radius: 0;
      cursor: pointer;
      overflow: hidden;
      box-shadow: 0 -1px rgba(255, 255, 255, 0.1);
    }

    header::before,
    .title::before {
      content: '';
      position: absolute;
      inset: -0.5rem -1rem 0;
      background: var(--background);
      background-size: 1px;
      filter: blur(0.5rem);
      mask: linear-gradient(to bottom, black, transparent);
      opacity: 0.2;
    }

    .title::before {
      mask: linear-gradient(to bottom right, black, transparent);
    }

    a a {
      display: none;
    }

    .active,
    a:has(.active) {
      color: var(--tui-text-primary);

      [tuiTitle] {
        font-weight: bold;
      }
    }

    hr {
      height: 1px;
      margin: 0.5rem;
      background: var(--tui-border-normal);
      border: none;
    }

    .section {
      text-transform: capitalize;
    }

    tui-data-list hr {
      flex: none;
      height: 1px;
      min-height: 0;
      margin: 0.25rem 0.5rem;
      padding: 0;
    }

    [tuiOption] {
      text-transform: capitalize;

      &.active {
        font-weight: bold;
      }
    }

    :host-context(tui-root._mobile) {
      flex-direction: column;

      aside {
        display: none;
      }
    }
  `,
  imports: [
    RouterModule,
    TuiCell,
    TuiDataList,
    TuiDropdown,
    TuiAvatar,
    TuiTitle,
    TuiAppearance,
    TuiIcon,
    TuiFade,
    TitleDirective,
    CornerDirective,
    TuiButton,
    i18nPipe,
  ],
})
export class ServiceOutletComponent {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly router = inject(Router)
  private readonly route = inject(ActivatedRoute)
  private readonly params = this.route.paramMap
  private readonly icons = inject(PackageIconService)

  protected readonly nav = computed((pkg = this.service()) =>
    [
      [
        {
          title: 'dashboard',
          icon: '@tui.layout-dashboard',
          link: './',
          exact: true,
        },
        {
          title: 'actions & config',
          icon: '@tui.cog',
          link: 'actions',
          exact: true,
        },
      ],
      [
        {
          title: 'interfaces',
          icon: '@tui.monitor',
          link: 'interfaces',
          exact: false,
        },
        {
          title: 'Outbound Traffic',
          icon: '@tui.globe',
          link: 'outbound-traffic',
          exact: true,
        },
      ],
      pkg && getCurrentManifest(pkg).sharedVolumes
        ? [
            {
              title: 'Shared Volumes',
              icon: '@tui.folder-symlink',
              link: 'volumes',
              exact: true,
            },
          ]
        : [],
      [
        { title: 'logs', icon: '@tui.scroll-text', link: 'logs', exact: true },
        {
          title: 'instructions',
          icon: '@tui.book-open-text',
          link: 'instructions',
          exact: true,
        },
        { title: 'about', icon: '@tui.info', link: 'about', exact: true },
      ],
    ].filter((group: NavItem[]) => group.length),
  )

  protected readonly menuOpen = signal(false)

  private readonly section = toSignal(
    this.router.events.pipe(
      filter(e => e instanceof NavigationEnd),
      startWith(null),
      map(() => this.route.firstChild?.snapshot?.url[0]?.path),
    ),
  )

  protected readonly current = computed(() =>
    this.nav()
      .flat()
      .find(({ link }) => link === this.section()),
  )

  protected readonly service = toSignal(
    this.router.events.pipe(
      switchMap(() => this.params),
      map(params => params.get('pkgId')),
      filter(Boolean),
      distinctUntilChanged(),
      switchMap(id => this.patch.watch$('packageData', id)),
      tap(pkg => {
        if (!pkg) {
          this.router.navigate(['services'])
        }
      }),
    ),
  )

  protected readonly icon = toSignal(
    toObservable(this.service).pipe(
      filter(Boolean),
      switchMap(pkg => this.icons.get(pkg)),
    ),
  )

  protected readonly manifest = computed(
    (pkg = this.service()) => pkg && getManifest(pkg),
  )
}
