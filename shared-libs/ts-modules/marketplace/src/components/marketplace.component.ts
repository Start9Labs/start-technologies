import { KeyValuePipe, NgTemplateOutlet } from '@angular/common'
import {
  Component,
  computed,
  contentChild,
  inject,
  input,
  model,
  signal,
  TemplateRef,
} from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import { WA_IS_MOBILE } from '@ng-web-apis/platform'
import {
  i18nPipe,
  knownRegistries as start9Registries,
  LocalizePipe,
  sameUrl,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TuiButton,
  tuiButtonOptionsProvider,
  TuiCell,
  TuiDataListComponent,
  TuiInput,
  TuiNotification,
  TuiOption,
  TuiOptionWithValue,
  TuiScrollbar,
  TuiTitle,
} from '@taiga-ui/core'
import {
  TuiAvatar,
  TuiButtonSelect,
  TuiChevron,
  TuiSkeleton,
} from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader, TuiNavigation } from '@taiga-ui/layout'

import { findKnown, identityMatches } from '../identity'
import { filterPackages } from '../pipes/filter-packages.pipe'
import { AbstractMarketplaceService } from '../services/abstract-marketplace.service'
import { StoreDataWithUrl } from '../types'

const ICONS: Record<string, string> = {
  all: '@tui.layout-grid',
  ai: '@tui.cpu',
  bitcoin: '@tui.bitcoin',
  crypto: '@tui.coins',
  'developer-tools': '@tui.code',
  finance: '@tui.chart-line',
  'files-and-productivity': '@tui.briefcase',
  'home-and-automation': '@tui.home',
  media: '@tui.circle-play',
  networking: '@tui.globe',
  nostr: '@tui.feather',
  social: '@tui.users',
}

@Component({
  selector: 'marketplace',
  template: `
    <aside [tuiNavigationAside]="open()">
      <ng-content />
      <tui-textfield tuiTextfieldSize="s" iconStart="@tui.search">
        <input
          tuiInput
          placeholder="Search"
          [(ngModel)]="query"
          (focus)="open.set(true)"
        />
      </tui-textfield>
      @for (cat of categories() || fallback | keyvalue: asIs; track cat.key) {
        <button
          [tuiSkeleton]="!categories()"
          [iconStart]="icons[cat.key.toLowerCase()] || '@tui.box'"
          [disabled]="!categories()"
          [class._active]="cat.key === effectiveCategory()"
          tuiAsideItem
          type="button"
          (click)="onCategory(cat.key)"
        >
          {{ cat.value.name ? (cat.value.name | localize) : cat.key }}
        </button>
      }
      <div class="actions">
        <ng-content select="[actions]" />
      </div>
      <footer>
        <button
          tuiAsideItem
          type="button"
          [iconStart]="open() ? '@tui.chevron-left' : '@tui.chevron-right'"
          (click)="open.set(!open())"
        >
          {{ open() ? 'Collapse' : 'Expand' }}
        </button>
      </footer>
    </aside>
    <div class="content">
      @if (thirdParty()) {
        <div tuiNotification appearance="warning">
          {{
            'Start9 does not operate this registry or support the services it distributes.'
              | i18n
          }}
        </div>
      }
      @if (drifted()) {
        <div tuiNotification appearance="negative">
          {{
            'This registry does not present the name and icon Start9 published for it. Start9 has not validated that it is what it claims to be.'
              | i18n
          }}
        </div>
      }
      <header tuiHeader="h4">
        <hgroup tuiTitle>
          <h2>
            @if (registry()) {
              {{ name() | localize }}
            }
          </h2>
        </hgroup>
        <aside tuiAccessories>
          <button
            appearance="secondary-grayscale"
            tuiButton
            tuiButtonSelect
            tuiChevron
            [(ngModel)]="sortLabel"
          >
            {{ sortLabel }}
            <tui-data-list *tuiDropdown>
              @for (key of sortKeys; track key) {
                <button tuiOption [value]="getLabel(key)">
                  {{ getLabel(key) }}
                </button>
              }
            </tui-data-list>
          </button>
        </aside>
      </header>
      <tui-scrollbar>
        <section>
          @if (registry()) {
            @for ($implicit of packages(); track $index) {
              <ng-container
                *ngTemplateOutlet="template(); context: { $implicit }"
              />
            }
          } @else {
            @for (_ of '-'.repeat(6); track $index) {
              <div tuiCardLarge="compact" [tuiSkeleton]="true">
                <span tuiCell>
                  <span tuiAvatar></span>
                  <span tuiTitle>
                    Loading
                    <span tuiSubtitle>Loading</span>
                  </span>
                </span>
                <span tuiDescription>Loading</span>
              </div>
            }
          }
        </section>
      </tui-scrollbar>
    </div>
  `,
  styles: `
    :host {
      --tui-theme-color: var(--tui-background-elevation-1);

      display: flex;
      width: 100%;

      aside {
        height: 100%;

        &::before {
          display: none;
        }

        tui-textfield {
          margin-bottom: 0.75rem;
          padding: 0 0.5rem;
          flex-wrap: nowrap;
          overflow: hidden;
          --t-start: 1.375rem;
        }
      }
    }

    .actions {
      display: grid;
      gap: 0.5rem;
      margin-block-start: 0.5rem;
    }

    .content {
      flex: 1;
      min-width: 0;
      display: flex;
      flex-direction: column;
      overflow: hidden;
    }

    [tuiNotification] {
      margin: 1rem 2rem 0;
    }

    [tuiHeader] {
      white-space: nowrap;
      padding: 1rem 2rem 0;
    }

    tui-scrollbar {
      padding-block-start: 1rem;
      mask: linear-gradient(transparent, black 1rem);
    }

    section {
      padding: 0 2rem 1rem;
      display: grid;
      gap: 1rem;
      grid-template-columns: repeat(auto-fill, minmax(17rem, 1fr));
    }

    :host-context(tui-root._mobile) {
      :is([tuiHeader], section) {
        padding-inline: 1rem;
      }

      [tuiNotification] {
        margin-inline: 1rem;
      }
    }
  `,
  providers: [
    tuiButtonOptionsProvider({ size: 's', appearance: 'flat-grayscale' }),
  ],
  imports: [
    TuiNavigation,
    TuiInput,
    TuiNotification,
    TuiSkeleton,
    TuiButton,
    TuiButtonSelect,
    TuiChevron,
    TuiDataListComponent,
    TuiHeader,
    TuiOption,
    TuiOptionWithValue,
    TuiScrollbar,
    TuiTitle,
    TuiAvatar,
    TuiCardLarge,
    TuiCell,
    NgTemplateOutlet,
    KeyValuePipe,
    LocalizePipe,
    FormsModule,
    i18nPipe,
  ],
})
export class MarketplaceComponent {
  readonly registry = input<StoreDataWithUrl>()
  readonly query = model('')
  readonly category = model('')

  protected readonly template = contentChild(TemplateRef)
  protected readonly sort = signal('a')
  protected readonly sortKeys = ['a', '1']
  protected readonly i18n = inject(i18nPipe)
  protected readonly icons = ICONS
  protected readonly asIs = () => 0
  protected readonly open = signal(!inject(WA_IS_MOBILE))
  private readonly known = toSignal(
    inject(AbstractMarketplaceService).knownRegistries$,
    { initialValue: [] },
  )

  protected readonly thirdParty = computed(() => {
    const url = this.registry()?.url

    return !!url && !Object.values(start9Registries).some(u => sameUrl(u, url))
  })

  protected readonly drifted = computed(() => {
    const registry = this.registry()
    const pin = registry && findKnown(registry.url, this.known())

    return !!pin && !identityMatches(pin, registry.info)
  })
  // Only categories that have at least one package are shown; 'all' is the
  // always-present pseudo-category injected by each app's service.
  protected readonly categories = computed(() => {
    const info = this.registry()?.info?.categories
    if (!info) return undefined

    const used = new Set(
      (this.registry()?.packages || []).flatMap(p => p.categories || []),
    )

    return Object.fromEntries(
      Object.entries(info).filter(([key]) => key === 'all' || used.has(key)),
    )
  })

  // The selected category may be absent from the current registry (e.g. after
  // switching registries); fall back to 'all' so the grid is never stuck empty.
  protected readonly effectiveCategory = computed(() => {
    const cats = this.categories()
    const cat = this.category()
    return cats && cat !== 'all' && !cats[cat] ? 'all' : cat
  })

  protected readonly name = computed(
    (c = this.effectiveCategory()) =>
      this.registry()?.info?.categories?.[c]?.name || c,
  )

  protected readonly packages = computed(() =>
    filterPackages(
      this.registry()?.packages || [],
      this.query(),
      this.effectiveCategory(),
      this.sort(),
    ),
  )

  protected readonly fallback: Record<string, T.Category> = {
    a: { name: '' },
    b: { name: '' },
    c: { name: '' },
  }

  protected get sortLabel(): string {
    return this.getLabel(this.sort())
  }

  protected set sortLabel(label: string) {
    this.sort.set(
      this.sortKeys.find(key => this.getLabel(key) === label) || 'a',
    )
  }

  onCategory(category: string) {
    this.query.set('')
    this.category.set(category)
  }

  getLabel(sort: string) {
    return this.i18n.transform(
      sort === 'a' ? 'Alphabetical' : 'Recently updated',
    )
  }
}
