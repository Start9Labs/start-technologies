import { Component, computed, inject, signal } from '@angular/core'
import { takeUntilDestroyed, toSignal } from '@angular/core/rxjs-interop'
import { ActivatedRoute, Router, RouterLink } from '@angular/router'
import {
  MarketplaceComponent,
  MarketplacePackageLinkComponent,
  MarketplaceRegistrySelectComponent,
  MarketplaceTileComponent,
  isRegistryLoading,
  StoreIdentity,
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
  concat,
  distinctUntilChanged,
  EMPTY,
  map,
  of,
  skip,
  startWith,
  switchMap,
  take,
  tap,
} from 'rxjs'
import { ConfigService } from 'src/app/services/config.service'
import { MarketplaceService } from 'src/app/services/marketplace.service'
import { StorageService } from 'src/app/services/storage.service'
import { TitleDirective } from 'src/app/services/title.service'
import { RegistryNotice } from 'src/app/routes/portal/components/registry-notice'

import { MarketplaceControlsComponent } from './components/controls.component'

@Component({
  template: `
    <ng-container *title>{{ 'Marketplace' | i18n }}</ng-container>
    <marketplace
      [registry]="registry()"
      [blocked]="invalidRegistry() || isRegistrySaved() === false"
      [(category)]="category"
      [(query)]="query"
    >
      <marketplace-registry-select manageRegistriesLink="/system/registries" />
      <a marketplacePackageLink actions></a>
      <ng-template let-pkg>
        <button [marketplaceTile]="pkg">
          <ng-template let-resolved>
            <marketplace-controls [pkg]="resolved" />
          </ng-template>
        </button>
      </ng-template>
      @if (invalidRegistry()) {
        <tui-block-status marketplaceBlocked>
          <h2>{{ 'Invalid registry URL' | i18n }}</h2>
          <p>
            {{
              'Registry URLs must use HTTP or HTTPS and cannot contain credentials, a query, or a fragment.'
                | i18n
            }}
          </p>
          <a
            tuiButton
            appearance="primary"
            size="l"
            routerLink="/system/registries"
          >
            {{ 'Manage Registries' | i18n }}
          </a>
        </tui-block-status>
      }
      @if (!invalidRegistry() && isRegistrySaved() === false) {
        <tui-block-status marketplaceBlocked>
          <h2>{{ 'Connect' | i18n }}</h2>
          <p>{{ selectedRegistry() }}</p>
          <p>
            {{
              'Connect this registry to browse its catalog. New signing keys from this registry will be trusted for packages.'
                | i18n
            }}
          </p>
          <button
            tuiButton
            appearance="primary"
            size="l"
            (click)="connectRegistry()"
          >
            {{ 'Connect' | i18n }}
          </button>
        </tui-block-status>
      }
      <registry-notice marketplaceNotice [url]="selectedRegistry()!" />
    </marketplace>
  `,
  host: { class: 'g-page' },
  styles: `
    registry-notice {
      margin: 1rem 2rem 0;
    }

    :host-context(tui-root._mobile) registry-notice {
      margin-inline: 1rem;
    }

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
    RegistryNotice,
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

  /** Automatic registry selection must not outlive the requesting route. */
  private readonly route = inject(ActivatedRoute)
    .queryParamMap.pipe(
      switchMap(params => {
        const registry = params.get('registry')

        if (params.has('search')) {
          this.query.set(params.get('search') || '')
        }

        if (!registry) {
          this.invalidRegistry.set(false)
          return this.marketplaceService.registries$.pipe(
            take(1),
            tap(saved => this.selectSaved(saved)),
          )
        }

        try {
          const url = registryUrl(registry)
          this.invalidRegistry.set(false)
          this.marketplaceService.currentRegistryUrl$.next(url)
        } catch {
          this.invalidRegistry.set(true)
        }
        return EMPTY
      }),
      takeUntilDestroyed(),
    )
    .subscribe()

  // Registry status changes must not reset loaded tiles or open previews.
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
              catchError((error, retry$) => {
                this.error.handleError(error)
                return concat(
                  of([]),
                  this.marketplaceService.currentRegistry$.pipe(
                    skip(1),
                    take(1),
                    switchMap(() => retry$),
                  ),
                )
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

  /** A remembered registry the server no longer saves gives way to the default. */
  private selectSaved(saved: StoreIdentity[]): void {
    const remembered = this.storage.get<string>('selectedRegistry')
    const registry =
      remembered && saved.some(({ url }) => sameUrl(url, remembered))
        ? remembered
        : this.configService.defaultRegistry
    if (remembered && registry !== remembered) {
      this.storage.set('selectedRegistry', null)
    }
    this.router.navigate([], {
      queryParams: { registry },
      queryParamsHandling: 'merge',
      replaceUrl: true,
    })
  }

  protected async connectRegistry(): Promise<void> {
    const url = this.selectedRegistry()!
    await this.tasks.run(async () => {
      await this.marketplaceService.add(url)
      await this.marketplaceService.connect(url)
    }, 'Validating registry')
  }
}
