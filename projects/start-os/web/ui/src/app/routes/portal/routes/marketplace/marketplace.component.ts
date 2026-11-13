import { CommonModule } from '@angular/common'
import { Component, inject, signal } from '@angular/core'
import { takeUntilDestroyed, toSignal } from '@angular/core/rxjs-interop'
import { FormsModule } from '@angular/forms'
import { ActivatedRoute, Router } from '@angular/router'
import {
  MarketplaceComponent,
  MarketplacePackageLinkComponent,
  MarketplaceRegistrySelectComponent,
  MarketplaceTileComponent,
} from '@start9labs/marketplace'
import { ErrorService, i18nPipe, registryUrl } from '@start9labs/shared'
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

import { MarketplaceControlsComponent } from './components/controls.component'

@Component({
  template: `
    <ng-container *title>{{ 'Marketplace' | i18n }}</ng-container>
    <marketplace
      [registry]="registry()"
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
    </marketplace>
  `,
  host: { class: 'g-page' },
  styles: `
    :host {
      display: flex;
      overflow: hidden;
      padding: 1px;
      --tui-background-base: #1c1d26;

      background: var(--tui-background-base);
    }
  `,
  imports: [
    CommonModule,
    FormsModule,
    TitleDirective,
    i18nPipe,
    MarketplaceComponent,
    MarketplaceControlsComponent,
    MarketplacePackageLinkComponent,
    MarketplaceRegistrySelectComponent,
    MarketplaceTileComponent,
  ],
})
export default class Marketplace {
  private readonly marketplaceService = inject(MarketplaceService)
  private readonly error = inject(ErrorService)
  private readonly configService = inject(ConfigService)
  private readonly router = inject(Router)
  private readonly storage = inject(StorageService)

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
          this.marketplaceService.currentRegistryUrl$.next(
            registryUrl(registry),
          )
        }
      }),
    )
    .subscribe()

  // Tile navigation does not restart provider discovery.
  protected readonly registry = toSignal(
    combineLatest([
      this.marketplaceService.currentRegistry$.pipe(distinctUntilChanged()),
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
      switchMap(([registry, [provides, range, providerId]]) => {
        return provides && range
          ? this.marketplaceService
              .fetchProviders$(registry.url, provides, range, providerId)
              .pipe(
                map(packages => ({ ...registry, packages })),
                catchError(error => {
                  this.error.handleError(error)
                  return of({ ...registry, packages: [] })
                }),
                startWith(undefined),
              )
          : of(registry)
      }),
    ),
  )
}
