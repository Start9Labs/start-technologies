import { Component, computed, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import {
  Marketplace,
  StoreIconDirective,
  StoreIdentity,
  isRegistryLoading,
} from '@start9labs/marketplace'
import { i18nPipe, isDefaultRegistry } from '@start9labs/shared'
import { TUI_BREAKPOINT, TuiButton, TuiCell, TuiTitle } from '@taiga-ui/core'
import {
  TuiAvatar,
  TuiBadgeNotification,
  TuiFade,
  TuiSkeleton,
} from '@taiga-ui/kit'
import { combineLatest } from 'rxjs'
import { UpdatesCount } from 'src/app/routes/portal/components/updates-count'
import { RegistryNotice } from 'src/app/routes/portal/components/registry-notice'
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
import { FilterUpdatesPipe } from './filter-updates.pipe'
import { UpdatesItemComponent } from './item.component'

interface UpdatesData {
  hosts: StoreIdentity[]
  marketplace: Marketplace
  localPkgs: Record<string, PackageDataEntry<InstalledState | UpdatingState>>
  hidden: Record<string, string[]>
}

@Component({
  template: `
    <ng-container *title>
      <label>{{ 'Updates' | i18n }}</label>
      @if (current()) {
        <button
          tuiIconButton
          iconStart="@tui.arrow-left"
          (click)="selectedRegistry.set('')"
        >
          {{ 'Back' | i18n }}
        </button>
        {{ current()?.name }}
      }
    </ng-container>
    <aside class="g-aside">
      @for (group of groups(); track group.title) {
        @if (group.title) {
          <h4 tuiTitle>{{ group.title | i18n }}</h4>
        }
        @for (registry of group.hosts; track registry.url) {
          <button
            tuiCell
            [class.g-secondary]="current()?.url !== registry.url"
            (click)="selectedRegistry.set(registry.url)"
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
                data()?.marketplace?.[registry.url]?.updates || []
                | filterUpdates: data()?.localPkgs : data()?.hidden
              ).length;
              as length
            ) {
              <tui-badge-notification appUpdatesCount>
                {{ length }}
              </tui-badge-notification>
            }
          </button>
        }
      }
    </aside>
    <section class="g-subpage">
      @if (current(); as registry) {
        @if (registry.status === 'error') {
          <registry-notice [url]="registry.url" />
        }
      }
      <section class="g-card">
        <header>
          {{ current()?.name }}
        </header>
        <table
          [appTable]="['Name', 'Version', 'Package Hash', 'Published', null]"
        >
          @let store = data()?.marketplace?.[current()?.url || ''];
          @if (
            store &&
            !isRegistryLoading(current()?.status, store.packages.length)
          ) {
            @if (
              store.updates || []
                | filterUpdates: data()?.localPkgs : data()?.hidden;
              as updates
            ) {
              @for (pkg of updates; track pkg.id) {
                <updates-item
                  [item]="pkg"
                  [local]="data()?.localPkgs?.[pkg.id]!"
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

    registry-notice {
      margin-block-end: 1rem;
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
    TuiSkeleton,
    TuiBadgeNotification,
    UpdatesCount,
    RegistryNotice,
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
  private readonly breakpoint = inject(TUI_BREAKPOINT)
  private readonly marketplaceService = inject(MarketplaceService)
  private readonly hiddenUpdatesService = inject(HiddenUpdatesService)

  protected readonly isRegistryLoading = isRegistryLoading
  protected readonly selectedRegistry = signal('')
  protected readonly groups = computed(() => {
    const hosts = this.data()?.hosts || []
    const standard = hosts.filter(({ url }) => isDefaultRegistry(url))
    const custom = hosts.filter(({ url }) => !isDefaultRegistry(url))
    return standard.length && custom.length
      ? [
          { title: 'Default Registries' as const, hosts: standard },
          { title: 'Custom Registries' as const, hosts: custom },
        ]
      : [{ title: null, hosts }]
  })
  readonly current = computed(() => {
    const hosts = this.data()?.hosts || []
    return (
      hosts.find(registry => registry.url === this.selectedRegistry()) ||
      (this.breakpoint() !== 'mobile' ? hosts[0] : null)
    )
  })

  readonly data = toSignal<UpdatesData>(
    combineLatest({
      hosts: this.marketplaceService.registries$,
      marketplace: this.marketplaceService.marketplace$,
      localPkgs: inject(LocalPackagesService),
      hidden: this.hiddenUpdatesService.effective$,
    }),
  )

  clear(url: string): string {
    return url.replace(/https?:\/\//, '').replace(/\/$/, '')
  }
}
