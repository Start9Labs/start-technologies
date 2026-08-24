import { Component, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import {
  getPkgId,
  i18nPipe,
  MarkdownPipe,
  SafeLinksDirective,
} from '@start9labs/shared'
import { TuiLoader, TuiNotification } from '@taiga-ui/core'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { catchError, defer, from, map, of, startWith } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'

@Component({
  template: `
    @if (value() === undefined) {
      <tui-loader textContent="Loading" [style.height.%]="100" />
    } @else if (value(); as value) {
      <article
        class="g-markdown"
        safeLinks
        [innerHTML]="value | markdown | dompurify"
      ></article>
    } @else {
      <div tuiNotification appearance="neutral">
        {{ 'This version has no instructions. Please update.' | i18n }}
      </div>
    }
  `,
  styles: `
    article {
      max-width: 48rem;
      font: var(--tui-typography-body-l);
    }
  `,
  host: { class: 'g-subpage' },
  imports: [
    NgDompurifyPipe,
    MarkdownPipe,
    SafeLinksDirective,
    TuiLoader,
    TuiNotification,
    i18nPipe,
  ],
})
export default class ServiceInstructionsRoute {
  protected readonly value = toSignal(
    from(
      inject(ApiService).getStatic(
        [`/s9pk/installed/${getPkgId()}.s9pk/instructions.md`],
        {},
      ),
    ).pipe(
      map(md => md?.trim()),
      catchError(() => of('')),
    ),
  )
}
