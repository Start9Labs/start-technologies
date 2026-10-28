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
import { i18nPipe } from '@start9labs/shared'
import { TuiButton, TuiDataList, TuiDropdown, TuiIcon } from '@taiga-ui/core'
import { tap } from 'rxjs'
import { ConfigService } from 'src/app/services/config.service'
import { MarketplaceService } from 'src/app/services/marketplace.service'
import { StorageService } from 'src/app/services/storage.service'
import { TrustedKeysService } from 'src/app/services/trusted-keys.service'
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
      <marketplace-registry-select />
      <a marketplacePackageLink actions></a>
      <button
        headerActions
        tuiIconButton
        tuiDropdown
        tuiDropdownAuto
        tuiDropdownAlign="end"
        appearance="icon"
        iconStart="@tui.ellipsis"
        size="s"
      >
        {{ 'More' | i18n }}
        <tui-data-list *tuiDropdown="let close">
          <button tuiOption (click)="close(); trust.manage()">
            <tui-icon icon="@tui.key-round" />
            {{ 'Manage trusted keyrings' | i18n }}
          </button>
        </tui-data-list>
      </button>
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
    TuiButton,
    TuiDataList,
    TuiDropdown,
    TuiIcon,
    MarketplaceComponent,
    MarketplaceControlsComponent,
    MarketplacePackageLinkComponent,
    MarketplaceRegistrySelectComponent,
    MarketplaceTileComponent,
  ],
})
export default class Marketplace {
  private readonly marketplaceService = inject(MarketplaceService)
  private readonly configService = inject(ConfigService)
  private readonly router = inject(Router)
  private readonly storage = inject(StorageService)
  private readonly route = inject(ActivatedRoute)
    .queryParamMap.pipe(
      takeUntilDestroyed(),
      tap(params => {
        const registry = params.get('registry')

        // Drawer query changes preserve the typed search.
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
          })
        } else {
          this.marketplaceService.currentRegistryUrl$.next(registry)
        }
      }),
    )
    .subscribe()

  protected readonly trust = inject(TrustedKeysService)
  protected readonly category = signal('all')
  protected readonly query = signal('')
  protected readonly registry = toSignal(
    this.marketplaceService.currentRegistry$,
  )
}
