import { Component, input } from '@angular/core'
import {
  LocalizePipe,
  MarkdownPipe,
  SafeLinksDirective,
} from '@start9labs/shared'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { MarketplacePkgBase } from '../types'

@Component({
  selector: 'marketplace-release-notes',
  template: `
    <header tuiHeader>New in {{ pkg().version }}</header>
    <div
      class="g-markdown"
      safeLinks
      [innerHTML]="pkg().releaseNotes | localize | markdown | dompurify"
    ></div>
  `,
  styles: `
    .g-markdown {
      overflow-wrap: anywhere;
    }
  `,
  hostDirectives: [TuiCardLarge],
  imports: [
    NgDompurifyPipe,
    MarkdownPipe,
    LocalizePipe,
    SafeLinksDirective,
    TuiHeader,
  ],
})
export class MarketplaceReleaseNotesComponent {
  readonly pkg = input.required<MarketplacePkgBase>()
}
