import { Component, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { WA_IS_MOBILE } from '@ng-web-apis/platform'
import {
  Marketplace,
  MarketplacePkg,
  StoreIconDirective,
  StoreIdentity,
} from '@start9labs/marketplace'
import { i18nPipe } from '@start9labs/shared'
import { TuiButton, TuiCell, TuiNotification, TuiTitle } from '@taiga-ui/core'
import {
  TuiAvatar,
  TuiBadgeNotification,
  TuiFade,
  TuiSkeleton,
} from '@taiga-ui/kit'
import { combineLatest, map, tap } from 'rxjs'
import { PlaceholderComponent } from 'src/app/routes/portal/components/placeholder.component'
import { TableComponent } from 'src/app/routes/portal/components/table.component'
import { HiddenUpdatesService } from 'src/app/services/hidden-updates.service'
import { LocalPackagesService } from 'src/app/services/local-packages.service'
import { MarketplaceService } from 'src/app/services/marketplace.service'
import {
  InstalledState,
  PackageDataEntry,
  UpdatingState,
} from 'src/app/services/patch-db/data-model'
import { TitleDirective } from 'src/app/services/title.service'
import {
  refinementKey,
  UpdatesRefinementService,
} from 'src/app/services/updates-refinement.service'
import { FilterUpdatesPipe } from './filter-updates.pipe'
import { UpdatesItemComponent } from './item.component'

interface UpdatesData {
  hosts: StoreIdentity[]
  marketplace: Marketplace
  localPkgs: Record<string, PackageDataEntry<InstalledState | UpdatingState>>
  errors: string[]
  hidden: Record<string, string[]>
  pending: Set<string>
}

@Component({
  template: `
    <ng-container *title>
      <label>{{ 'Updates' | i18n }}</label>
      @if (current()) {
        <button
          tuiIconButton
          iconStart="@tui.arrow-left"
          (click)="current.set(null)"
        >
          {{ 'Back' | i18n }}
        </button>
        {{ current()?.name }}
      }
    </ng-container>
    <aside class="g-aside">
      @if ((data()?.hosts || []).length > 2) {
        <h4 tuiTitle>{{ 'Default Registries' | i18n }}</h4>
      }
      @for (registry of data()?.hosts; track $index) {
        @if ($index === 2 && (data()?.hosts || []).length > 2) {
          <h4 tuiTitle>{{ 'Custom Registries' | i18n }}</h4>
        }
        <button
          tuiCell
          [class.g-secondary]="current()?.url !== registry.url"
          (click)="current.set(registry)"
        >
          <span tuiAvatar appearance="action-grayscale">
            <img [storeIcon]="registry.url" />
          </span>
          <span tuiTitle>
            <b tuiFade>{{ registry.name }}</b>
            <span tuiSubtitle tuiFade>{{ clear(registry.url) }}</span>
          </span>
          @if (
            (
              data()?.marketplace?.[registry.url]?.packages || []
              | filterUpdates: data()?.localPkgs : data()?.hidden
            ).length;
            as length
          ) {
            <tui-badge-notification>
              {{ length }}
            </tui-badge-notification>
          }
        </button>
      }
    </aside>
    <section class="g-subpage">
      @if (data()?.errors?.includes(current()?.url || '')) {
        <div
          tuiNotification
          appearance="negative"
          [style.margin-block-end.rem]="1"
        >
          {{ 'Request failed' | i18n }}
        </div>
      }
      <section class="g-card">
        <header>{{ current()?.name }}</header>
        <table
          [appTable]="['Name', 'Version', 'Package Hash', 'Published', null]"
        >
          @if (
            data()?.marketplace?.[current()?.url || '']?.packages;
            as packages
          ) {
            @if (
              packages | filterUpdates: data()?.localPkgs : data()?.hidden;
              as updates
            ) {
              @for (pkg of updates; track pkg.id) {
                <updates-item
                  [item]="pkg"
                  [local]="data()?.localPkgs?.[pkg.id]!"
                  [pending]="isPending(pkg)"
                />
              } @empty {
                <tr>
                  <td colspan="5">
                    <app-placeholder icon="@tui.circle-check">
                      {{ 'All services are up to date!' | i18n }}
                    </app-placeholder>
                  </td>
                </tr>
              }
            }
          } @else {
            <tr>
              <td colspan="5" [tuiSkeleton]="true">{{ 'Loading' | i18n }}</td>
            </tr>
            <tr>
              <td colspan="5" [tuiSkeleton]="true">{{ 'Loading' | i18n }}</td>
            </tr>
          }
        </table>
      </section>
    </section>
  `,
  styles: `
    :host {
      display: flex;
      padding: 0;
    }

    aside {
      display: flex;
      flex-direction: column;
      gap: 0.5rem;
      padding-top: 1rem;
      color: var(--tui-text-secondary);
    }

    h4 {
      color: var(--tui-text-primary);
      padding: 0.25rem 1rem;
      margin-top: 1rem;
      font-weight: bold;

      &:first-child {
        margin-top: 0;
      }
    }

    label:not(:last-child) {
      display: none;
    }

    [tuiCell] {
      white-space: nowrap;

      &:not(.g-secondary) {
        background: var(--tui-background-neutral-1);
        box-shadow: inset 0 0 0 1px var(--tui-background-neutral-1-hover);
        color: var(--tui-text-primary);
      }
    }

    td {
      clip-path: inset(0.5rem round var(--tui-radius-s));
    }

    .g-subpage,
    .g-card {
      overflow: auto;
    }

    :host-context(tui-root._mobile) {
      aside {
        width: 100%;
      }

      section {
        background: none;
        box-shadow: none;
        padding: 0.5rem;
      }

      header {
        display: none;
      }

      [tuiCell] {
        color: var(--tui-text-primary) !important;
      }

      :host._selected {
        aside {
          display: none;
        }
      }

      :host:not(._selected) {
        section {
          display: none;
        }
      }
    }
  `,
  host: {
    class: 'g-page',
    '[class._selected]': 'current()',
  },
  imports: [
    TuiCell,
    TuiAvatar,
    TuiTitle,
    TuiNotification,
    TuiSkeleton,
    TuiBadgeNotification,
    TuiFade,
    TuiButton,
    StoreIconDirective,
    FilterUpdatesPipe,
    UpdatesItemComponent,
    TitleDirective,
    TableComponent,
    i18nPipe,
    PlaceholderComponent,
  ],
})
export default class UpdatesComponent {
  private readonly isMobile = inject(WA_IS_MOBILE)
  private readonly marketplaceService = inject(MarketplaceService)
  private readonly hiddenUpdatesService = inject(HiddenUpdatesService)
  private readonly refinementService = inject(UpdatesRefinementService)

  readonly current = signal<StoreIdentity | null>(null)

  // The registry's "best" version can be unreachable from the user's installed
  // version — e.g. NextCloud major updates must be sequential, so v30 users
  // can't jump straight to v32. UpdatesRefinementService runs this resolution
  // in the background (kept warm by BadgeService's navbar subscription) so by
  // the time the user opens this tab the refined state is usually already
  // resolved and replayed via shareReplay — no "Finding suitable version..."
  // flash on each navigation.
  readonly data = toSignal<UpdatesData>(
    combineLatest({
      hosts: this.marketplaceService.registries$.pipe(
        tap(
          ([registry]) =>
            !this.isMobile && registry && this.current.set(registry),
        ),
      ),
      refined: this.refinementService.refined$,
      localPkgs: inject(LocalPackagesService),
      errors: this.marketplaceService.requestErrors$,
      hidden: this.hiddenUpdatesService.effective$,
    }).pipe(
      map(({ hosts, refined, localPkgs, errors, hidden }) => ({
        hosts,
        marketplace: refined.marketplace,
        pending: refined.pending,
        localPkgs,
        errors,
        hidden,
      })),
    ),
  )

  clear(url: string): string {
    return url.replace(/https?:\/\//, '').replace(/\/$/, '')
  }

  isPending(pkg: MarketplacePkg): boolean {
    const url = this.current()?.url
    if (!url) return false
    return (
      this.data()?.pending?.has(refinementKey(url, pkg.id, pkg.flavor)) ?? false
    )
  }
}
