import { Component, input } from '@angular/core'
import {
  i18nPipe,
  LocalizePipe,
  MarkdownPipe,
  SafeLinksDirective,
} from '@start9labs/shared'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { TuiBadge } from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { MarketplacePkgBase } from '../types'

@Component({
  selector: 'marketplace-release-notes',
  template: `
    <header tuiHeader>New in {{ pkg().version }}</header>
    @if (pkg().security) {
      <span tuiBadge appearance="negative" iconStart="@tui.shield-alert">
        {{ 'Security update' | i18n }}
      </span>
    }
    <div
      class="g-markdown"
      safeLinks
      [innerHTML]="
        pkg().releaseNotes
          | localize: pkg().defaultLocale
          | markdown
          | dompurify
      "
    ></div>
  `,
  styles: `
    .g-markdown {
      overflow-wrap: anywhere;
    }

    [tuiBadge] {
      align-self: start;
    }
  `,
  hostDirectives: [TuiCardLarge],
  imports: [
    NgDompurifyPipe,
    MarkdownPipe,
    LocalizePipe,
    SafeLinksDirective,
    TuiBadge,
    TuiHeader,
    i18nPipe,
  ],
})
export class MarketplaceReleaseNotesComponent {
  readonly pkg = input.required<MarketplacePkgBase>()
}
