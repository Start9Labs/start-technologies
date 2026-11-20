import { Component, inject, input } from '@angular/core'
import { toObservable, toSignal } from '@angular/core/rxjs-interop'
import {
  i18nPipe,
  LocalizePipe,
  MarkdownPipe,
  SafeLinksDirective,
} from '@start9labs/shared'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { TuiBadge } from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { switchMap } from 'rxjs'
import { ReleasesPipe } from '../pipes/releases.pipe'
import { AbstractMarketplaceService } from '../services/abstract-marketplace.service'
import { GetPackageRes, MarketplacePkgBase } from '../types'

@Component({
  selector: 'marketplace-release-notes',
  template: `
    @for (release of pkg() | releases: installed(); track release.version) {
      <header tuiHeader>New in {{ release.version }}</header>
      @if (release.security) {
        <span tuiBadge appearance="negative" iconStart="@tui.shield-alert">
          {{ 'Security update' | i18n }}
        </span>
      }
      <div
        class="g-markdown"
        safeLinks
        [innerHTML]="
          release.releaseNotes
            | localize: release.defaultLocale
            | markdown
            | dompurify
        "
      ></div>
    }
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
    ReleasesPipe,
    SafeLinksDirective,
    TuiBadge,
    TuiHeader,
    i18nPipe,
  ],
})
export class MarketplaceReleaseNotesComponent {
  private readonly marketplace = inject(AbstractMarketplaceService)

  readonly pkg = input.required<
    MarketplacePkgBase & Partial<Pick<GetPackageRes, 'otherVersions'>>
  >()

  protected readonly installed = toSignal(
    toObservable(this.pkg).pipe(
      switchMap(({ id }) => this.marketplace.installedVersion$(id)),
    ),
  )
}
