import { Component } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { TuiLoader, TuiNotification } from '@taiga-ui/core'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { catchError, ignoreElements, Observable, of, share } from 'rxjs'
import { SafeLinksDirective } from '../directives/safe-links.directive'
import { MarkdownPipe } from '../pipes/markdown.pipe'
import { getErrorMessage } from '../services/error.service'

@Component({
  template: `
    @if (error()) {
      <div
        tuiNotification
        appearance="negative"
        safeLinks
        [innerHTML]="error()"
      ></div>
    }

    @if (content(); as result) {
      <div
        class="g-markdown"
        safeLinks
        [innerHTML]="result | markdown | dompurify"
      ></div>
    } @else {
      @if (!error()) {
        <tui-loader textContent="Loading" [style.height.%]="100" />
      }
    }
  `,
  host: { class: 'g-subpage' },
  imports: [
    TuiNotification,
    TuiLoader,
    NgDompurifyPipe,
    MarkdownPipe,
    SafeLinksDirective,
  ],
})
export class MarkdownComponent {
  private readonly data = injectContext<{
    data: Observable<string>
  }>().data.pipe(share())

  protected readonly content = toSignal<string>(this.data)
  protected readonly error = toSignal(
    this.data.pipe(
      ignoreElements(),
      catchError(e => of(getErrorMessage(e))),
    ),
  )
}

export const MARKDOWN = new PolymorpheusComponent(MarkdownComponent)
