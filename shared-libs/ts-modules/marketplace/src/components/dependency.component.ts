import { KeyValue } from '@angular/common'
import { Component, inject, input } from '@angular/core'
import { RouterModule } from '@angular/router'
import { i18nPipe, i18nService, LocalizePipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiCell, TuiTitle } from '@taiga-ui/core'
import { TuiAvatar } from '@taiga-ui/kit'
import { MarketplacePkgBase } from '../types'
import { MarketplaceIconDirective } from './icon.directive'

@Component({
  selector: '[marketplaceDependency]',
  template: `
    <span tuiAvatar appearance="action-grayscale" [round]="false">
      <img [marketplaceIcon]="pkg()" [dependency]="dep().key" />
    </span>
    <span tuiTitle>
      {{ getTitle(dep().key) }}
      @if (dep().value.optional) {
        ({{ 'Optional' | i18n }})
      } @else {
        ({{ 'Required' | i18n }})
      }
      @if (dep().value.description; as description) {
        <span tuiSubtitle>{{ description | localize }}</span>
      }
    </span>
  `,
  hostDirectives: [TuiCell],
  imports: [
    RouterModule,
    TuiAvatar,
    i18nPipe,
    LocalizePipe,
    TuiTitle,
    MarketplaceIconDirective,
  ],
})
export class MarketplaceDependencyComponent {
  private readonly i18nService = inject(i18nService)

  readonly pkg = input.required<MarketplacePkgBase>()
  readonly dep = input.required<KeyValue<string, T.DependencyMetadata>>()

  getTitle(key: string): string {
    const title = this.pkg().dependencyMetadata[key]?.title
    return title ? this.i18nService.localize(title) : key
  }
}
