import {
  Component,
  computed,
  contentChild,
  inject,
  input,
  TemplateRef,
} from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { ActivatedRoute, Router } from '@angular/router'
import { i18nPipe, LocalizePipe } from '@start9labs/shared'
import { TuiButtonX, TuiCell, TuiPopup, TuiTitle } from '@taiga-ui/core'
import { TuiAvatar, TuiDrawer } from '@taiga-ui/kit'
import { TuiCardLarge, tuiCardOptionsProvider } from '@taiga-ui/layout'
import { debounceTime } from 'rxjs'

import { MarketplacePkg } from '../types'
import { MarketplaceIconDirective } from './icon.directive'
import { MarketplacePreviewComponent } from './preview.component'

@Component({
  selector: 'button[marketplaceTile]',
  template: `
    <span tuiCell>
      <span tuiAvatar [round]="false">
        <img [marketplaceIcon]="pkg()" />
      </span>
      <span tuiTitle>
        <b>{{ pkg().title }}</b>
        <span tuiSubtitle>{{ pkg().version }}</span>
      </span>
    </span>
    <span tuiDescription>{{ pkg().description.short | localize }}</span>
    <tui-drawer
      *tuiPopup="open()"
      [overlay]="true"
      (click.self)="toggle(false)"
    >
      <marketplace-preview [pkgId]="pkg().id" [controls]="controls()">
        <button tuiButtonX [tuiAppearanceFocus]="false" (click)="toggle(false)">
          {{ 'Close' | i18n }}
        </button>
      </marketplace-preview>
    </tui-drawer>
  `,
  styles: `
    :host {
      text-align: start;
      box-shadow: none !important;
    }

    [tuiDescription] {
      margin: 0 0 -0.25rem !important;
      color: var(--tui-text-secondary);
      display: -webkit-box;
      -webkit-box-orient: vertical;
      -webkit-line-clamp: 2;
      overflow: hidden;
    }

    tui-drawer {
      top: 0;
      width: 28rem;
      border-radius: 0;
    }
  `,
  host: {
    type: 'button',
    '(click)': 'toggle(true)',
  },
  hostDirectives: [TuiCardLarge],
  providers: [
    tuiCardOptionsProvider({ space: 'compact', appearance: 'floating' }),
  ],
  imports: [
    TuiPopup,
    TuiDrawer,
    MarketplacePreviewComponent,
    LocalizePipe,
    TuiAvatar,
    TuiTitle,
    TuiCell,
    TuiButtonX,
    i18nPipe,
    MarketplaceIconDirective,
  ],
})
export class MarketplaceTileComponent {
  private readonly router = inject(Router)
  private readonly params = toSignal(
    inject(ActivatedRoute).queryParamMap.pipe(debounceTime(100)),
  )

  readonly pkg = input.required<MarketplacePkg>({ alias: 'marketplaceTile' })

  /** Optional action-button template (install/update), projected by the
   * consumer and forwarded to the preview. Omitted by the brochure. */
  readonly controls =
    contentChild<TemplateRef<{ $implicit: MarketplacePkg }>>(TemplateRef)

  readonly open = computed(
    () =>
      this.params()?.get('id') === this.pkg()?.id &&
      this.params()?.get('flavor') === this.pkg()?.flavor,
  )

  toggle(open: boolean) {
    this.router.navigate([], {
      queryParams: {
        id: open ? this.pkg().id : null,
        flavor: open ? this.pkg().flavor : null,
      },
      queryParamsHandling: 'merge',
    })
  }
}
