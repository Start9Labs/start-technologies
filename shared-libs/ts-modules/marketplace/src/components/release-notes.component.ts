import { Component, input } from '@angular/core'
import { LocalizePipe, MarkdownPipe } from '@start9labs/shared'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { MarketplacePkgBase } from '../types'

@Component({
  selector: 'marketplace-release-notes',
  template: `
    <header tuiHeader>New in {{ pkg().version }}</header>
    <div
      [innerHTML]="
        pkg().releaseNotes
          | localize: pkg().defaultLocale
          | markdown
          | dompurify
      "
    ></div>
  `,
  styles: `
    :host {
      max-block-size: 30rem;
      scrollbar-width: none;
      overflow: auto;
    }
  `,
  hostDirectives: [TuiCardLarge],
  imports: [NgDompurifyPipe, MarkdownPipe, LocalizePipe, TuiHeader],
})
export class MarketplaceReleaseNotesComponent {
  readonly pkg = input.required<MarketplacePkgBase>()
}
