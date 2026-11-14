import { Component, computed, inject, signal } from '@angular/core'
import { takeUntilDestroyed, toSignal } from '@angular/core/rxjs-interop'
import { ActivatedRoute, Router, RouterLink } from '@angular/router'
import {
  MarketplaceComponent,
  MarketplacePackageLinkComponent,
  MarketplaceRegistrySelectComponent,
  MarketplaceTileComponent,
  isRegistryLoading,
} from '@start9labs/marketplace'
import {
  ErrorService,
  i18nPipe,
  registryUrl,
  sameUrl,
  TaskService,
} from '@start9labs/shared'
import { TuiButton } from '@taiga-ui/core'
import { TuiBlockStatus } from '@taiga-ui/layout'
import {
  catchError,
  combineLatest,
  distinctUntilChanged,
  map,
  of,
  startWith,
  switchMap,
  tap,
} from 'rxjs'
import { ConfigService } from 'src/app/services/config.service'
import { MarketplaceService } from 'src/app/services/marketplace.service'
import { StorageService } from 'src/app/services/storage.service'
import { TitleDirective } from 'src/app/services/title.service'
import { RegistryRetry } from 'src/app/routes/portal/components/registry-retry'

import { MarketplaceControlsComponent } from './components/controls.component'

@Component({
  template: `
    <ng-container *title>{{ 'Marketplace' | i18n }}</ng-container>
    @if (invalidRegistry()) {
      <tui-block-status>
        <h2>{{ 'Invalid registry URL' | i18n }}</h2>
        <p>
          {{
            'Registry URLs must use HTTP or HTTPS and cannot contain credentials, a query, or a fragment.'
              | i18n
          }}
        </p>
        <a tuiButton routerLink="/system/registries">
          {{ 'Manage Registries' | i18n }}
        </a>
      </tui-block-status>
    } @else if (isRegistrySaved() === false) {
      <tui-block-status>
        <h2>{{ 'Connect' | i18n }}</h2>
        <p>{{ selectedRegistry() }}</p>
        <p>
          {{
            'Connect this registry to browse its catalog. New signing keys from this registry will be trusted for packages.'
              | i18n
          }}
        </p>
        <button tuiButton (click)="connectRegistry()">
          {{ 'Connect' | i18n }}
        </button>
      </tui-block-status>
    } @else {
      <marketplace
        [registry]="registry()"
        [(category)]="category"
        [(query)]="query"
      >
        <marketplace-registry-select
          manageRegistriesLink="/system/registries"
        />
        <a marketplacePackageLink actions></a>
        <ng-template let-pkg>
          <button [marketplaceTile]="pkg">
            <ng-template let-resolved>
              <marketplace-controls [pkg]="resolved" />
            </ng-template>
          </button>
        </ng-template>
        <button [registryRetry]="selectedRegistry()!" size="xs"></button>
      </marketplace>
    }
  `,
  host: { class: 'g-page' },
  styles: `
    :host {
      display: flex;
      overflow: hidden;
      padding: 1px;
      background: var(--tui-background-base);
    }
  `,
  imports: [
    TuiButton,
    TuiBlockStatus,
    RouterLink,
    TitleDirective,
    i18nPipe,
    MarketplaceComponent,
    MarketplaceControlsComponent,
    MarketplacePackageLinkComponent,
    MarketplaceRegistrySelectComponent,
    MarketplaceTileComponent,
    RegistryRetry,
  ],
})
export default class Marketplace {
  private readonly marketplaceService = inject(MarketplaceService)
  private readonly error = inject(ErrorService)
  private readonly configService = inject(ConfigService)
  private readonly router = inject(Router)
  private readonly storage = inject(StorageService)
  private readonly tasks = inject(TaskService)

  protected readonly invalidRegistry = signal(false)
  protected readonly selectedRegistry = toSignal(
    this.marketplaceService.currentRegistryUrl$,
  )
  private readonly savedRegistries = toSignal(
    this.marketplaceService.registries$,
  )
  protected readonly isRegistrySaved = computed(() => {
    const registry = this.selectedRegistry()
    return (
      registry &&
      this.savedRegistries()?.some(saved => sameUrl(saved.url, registry))
    )
  })
  protected readonly category = signal('all')
  protected readonly query = signal('')

  private readonly route = inject(ActivatedRoute)
    .queryParamMap.pipe(
      takeUntilDestroyed(),
      tap(params => {
        const registry = params.get('registry')

        if (params.has('search')) {
          this.query.set(params.get('search') || '')
        }

        if (!registry) {
          this.invalidRegistry.set(false)
          this.router.navigate([], {
            queryParams: {
              registry:
                this.storage.get('selectedRegistry') ||
                this.configService.defaultRegistry,
            },
            queryParamsHandling: 'merge',
            replaceUrl: true,
          })
        } else {
          try {
            const url = registryUrl(registry)
            this.invalidRegistry.set(false)
            this.marketplaceService.currentRegistryUrl$.next(url)
          } catch {
            this.invalidRegistry.set(true)
          }
        }
      }),
    )
    .subscribe()

  // Only the registry URL and provider query restart discovery.
  private readonly providers$ = combineLatest([
    this.marketplaceService.currentRegistry$.pipe(
      map(({ url }) => url),
      distinctUntilChanged(),
    ),
    inject(ActivatedRoute).queryParamMap.pipe(
      map(params => [
        params.get('provides'),
        params.get('requireRange'),
        params.get('providerId'),
      ]),
      distinctUntilChanged(
        ([a, b, c], [d, e, f]) => a === d && b === e && c === f,
      ),
    ),
  ]).pipe(
    switchMap(([url, [provides, range, providerId]]) =>
      provides && range
        ? this.marketplaceService
            .fetchProviders$(url, provides, range, providerId)
            .pipe(
              catchError(error => {
                this.error.handleError(error)
                return of([])
              }),
              startWith(null),
            )
        : of(undefined),
    ),
  )

  protected readonly registry = toSignal(
    combineLatest([
      this.marketplaceService.currentRegistry$,
      this.providers$,
    ]).pipe(
      map(([registry, packages]) =>
        packages === undefined
          ? registry
          : {
              ...registry,
              packages: packages || [],
              loading:
                !packages ||
                isRegistryLoading(registry.status, registry.packages.length),
            },
      ),
    ),
  )

  protected async connectRegistry(): Promise<void> {
    const url = this.selectedRegistry()!
    await this.tasks.run(async () => {
      await this.marketplaceService.add(url)
      await this.marketplaceService.connect(url)
    }, 'Validating registry')
  }
}
