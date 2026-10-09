import { Component, computed, inject } from '@angular/core'
import { toObservable, toSignal } from '@angular/core/rxjs-interop'
import { ActivatedRoute, Router, RouterModule } from '@angular/router'
import { i18nKey, i18nPipe } from '@start9labs/shared'
import {
  TuiAppearance,
  TuiButton,
  TuiCell,
  TuiIcon,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiAvatar, TuiFade } from '@taiga-ui/kit'
import { PatchDB } from 'patch-db-client'
import { distinctUntilChanged, filter, map, switchMap, tap } from 'rxjs'
import { PackageIconService } from 'src/app/services/package-icon.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { TitleDirective } from 'src/app/services/title.service'
import { getManifest } from 'src/app/utils/get-package-data'

// `exact` gates routerLinkActive: the dashboard ('./') must match exactly or it
// would light up on every sub-route; `interfaces` is inexact so it stays active
// on its detail sub-pages (interfaces/:id). Leaf routes are exact.
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
        <a routerLink=".." tuiIconButton iconStart="@tui.arrow-left">
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
        <span tuiFade>{{ manifest()?.title }}</span>
      </div>
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
          @for (item of nav; track $index) {
            <a
              tuiCell
              tuiAppearance="action-grayscale"
              routerLinkActive="active"
              [routerLinkActiveOptions]="{ exact: item.exact }"
              [routerLink]="item.link"
            >
              <tui-icon [icon]="item.icon" />
              <span tuiTitle>{{ item.title | i18n }}</span>
            </a>
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
      margin: -0.5rem -0.5rem 0;
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

    :host-context(tui-root._mobile) {
      flex-direction: column;
      padding: 0;

      aside {
        top: 0;
        left: 0;
        width: 100%;
        padding: 0;
        margin: 0;
        z-index: 1;
        box-shadow: inset 0 1px 0 1px var(--tui-background-neutral-1);

        header {
          display: none;
        }

        nav {
          display: flex;
        }

        a {
          flex: 1;
          justify-content: center;
          border-radius: 0;
          padding: 0.125rem;
          background: var(--tui-background-neutral-1);
          box-shadow: inset 0 -1px var(--tui-background-neutral-1);

          &.active,
          &:has(.active) {
            background: none;
            box-shadow: none;
          }

          [tuiTitle],
          tui-icon:last-child {
            display: none;
          }
        }
      }
    }
  `,
  imports: [
    RouterModule,
    TuiCell,
    TuiAvatar,
    TuiTitle,
    TuiAppearance,
    TuiIcon,
    TuiFade,
    TitleDirective,
    TuiButton,
    i18nPipe,
  ],
})
export class ServiceOutletComponent {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly router = inject(Router)
  private readonly params = inject(ActivatedRoute).paramMap
  private readonly icons = inject(PackageIconService)

  protected readonly nav: NavItem[] = [
    {
      title: 'dashboard',
      icon: '@tui.layout-dashboard',
      link: './',
      exact: true,
    },
    {
      title: 'interfaces',
      icon: '@tui.monitor',
      link: 'interfaces',
      exact: false,
    },
    {
      title: 'instructions',
      icon: '@tui.book-open-text',
      link: 'instructions',
      exact: true,
    },
    { title: 'logs', icon: '@tui.scroll-text', link: 'logs', exact: true },
    { title: 'about', icon: '@tui.info', link: 'about', exact: true },
    {
      title: 'actions & config',
      icon: '@tui.cog',
      link: 'actions',
      exact: true,
    },
  ]

  protected readonly service = toSignal(
    this.router.events.pipe(
      switchMap(() => this.params),
      map(params => params.get('pkgId')),
      filter(Boolean),
      distinctUntilChanged(),
      switchMap(id => this.patch.watch$('packageData', id)),
      tap(pkg => {
        // if package disappears, navigate to list page
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
