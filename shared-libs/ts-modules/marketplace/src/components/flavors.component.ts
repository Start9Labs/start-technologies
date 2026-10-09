import { Component, input, Input } from '@angular/core'
import { RouterLink } from '@angular/router'
import { i18nPipe } from '@start9labs/shared'
import { TuiTitle, TuiCell } from '@taiga-ui/core'
import { TuiAvatar } from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { MarketplacePkg } from '../types'
import { MarketplaceIconDirective } from './icon.directive'

@Component({
  selector: 'marketplace-flavors',
  template: `
    <header tuiHeader>{{ 'Alternative Implementations' | i18n }}</header>
    @for (pkg of pkgs(); track $index) {
      <a
        tuiCell
        queryParamsHandling="merge"
        [routerLink]="[]"
        [queryParams]="{ id: pkg.id, flavor: pkg.flavor }"
      >
        <span tuiAvatar appearance="action-grayscale" [round]="false">
          <img [marketplaceIcon]="pkg" />
        </span>
        <span tuiTitle>
          {{ pkg.title }}
          <span tuiSubtitle>{{ pkg.version }}</span>
        </span>
      </a>
    }
  `,
  hostDirectives: [TuiCardLarge],
  imports: [
    RouterLink,
    TuiCell,
    TuiTitle,
    MarketplaceIconDirective,
    TuiAvatar,
    i18nPipe,
    TuiHeader,
  ],
})
export class MarketplaceFlavorsComponent {
  readonly pkgs = input.required<MarketplacePkg[]>()
}
