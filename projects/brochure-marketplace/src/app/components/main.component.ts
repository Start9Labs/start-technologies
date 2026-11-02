import {
  ChangeDetectionStrategy,
  Component,
  inject,
  signal,
} from '@angular/core'
import { takeUntilDestroyed, toSignal } from '@angular/core/rxjs-interop'
import { ActivatedRoute, Router } from '@angular/router'
import {
  MarketplaceComponent,
  MarketplacePackageLinkComponent,
  MarketplaceTileComponent,
} from '@start9labs/marketplace'
import {
  defaultRegistries,
  DialogService,
  i18nKey,
  i18nPipe,
  sameUrl,
} from '@start9labs/shared'
import { TuiButton } from '@taiga-ui/core'

import { MarketplaceService } from 'src/app/services/marketplace.service'

@Component({
  template: `
    <marketplace
      [registry]="registry()"
      [(category)]="category"
      [(query)]="query"
    >
      <a
        actions
        tuiButton
        iconStart="@tui.shopping-cart"
        appearance="primary-success"
        target="_blank"
        rel="noreferrer"
        href="https://start9.com"
      >
        {{ 'Get a Start9 server' | i18n }}
      </a>
      <a marketplacePackageLink actions></a>
      <ng-template let-pkg>
        <button [marketplaceTile]="pkg"></button>
      </ng-template>
    </marketplace>
  `,
  styles: `
    marketplace {
      height: 100%;

      [tuiButton] {
        justify-content: flex-start;
        gap: 0.625rem;
        font-weight: bold;
      }
    }
  `,
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    MarketplaceComponent,
    MarketplacePackageLinkComponent,
    MarketplaceTileComponent,
    TuiButton,
    i18nPipe,
  ],
})
export class MainComponent {
  private readonly router = inject(Router)
  private readonly route = inject(ActivatedRoute)
  private readonly service = inject(MarketplaceService)
  private readonly dialog = inject(DialogService)
  private readonly i18n = inject(i18nPipe)

  protected readonly category = signal('all')
  protected readonly query = signal('')
  protected readonly registry = toSignal(this.service.currentRegistry$)

  constructor() {
    // Registry, service drawer (id/flavor) and search all live in query params,
    // so any link is shareable: ?registry=<url>&id=<pkg>&flavor=<f>&search=<q>
    this.route.queryParamMap.pipe(takeUntilDestroyed()).subscribe(params => {
      const registry = params.get('registry')

      // Only override the search box when `search` is explicitly present, so
      // opening a drawer (which only adds id/flavor) doesn't wipe the query.
      if (params.has('search')) {
        this.query.set(params.get('search') || '')
      }

      if (!registry) {
        this.router.navigate([], {
          queryParams: {
            registry:
              this.service.lastSelectedRegistry || defaultRegistries.start9,
          },
          queryParamsHandling: 'merge',
        })
      } else {
        this.service.currentRegistryUrl$.next(registry)
      }
    })

    // Surface unreachable registries and fall back to the default when the
    // failed registry is the one requested in the URL.
    this.service.registryError$.pipe(takeUntilDestroyed()).subscribe(url => {
      this.dialog
        .openAlert(
          `${this.i18n.transform('Could not reach registry')}: ${url}` as i18nKey,
          { label: 'Error' },
        )
        .subscribe()

      const current = this.route.snapshot.queryParamMap.get('registry')
      if (
        current &&
        sameUrl(current, url) &&
        !sameUrl(url, defaultRegistries.start9)
      ) {
        this.router.navigate([], {
          queryParams: {
            registry: defaultRegistries.start9,
            id: null,
            flavor: null,
          },
          queryParamsHandling: 'merge',
        })
      }
    })
  }
}
