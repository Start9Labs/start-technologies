import { Directive, DOCUMENT, inject } from '@angular/core'
import { takeUntilDestroyed } from '@angular/core/rxjs-interop'
import {
  provideMutationObserverInit,
  WaMutationObserverService,
} from '@ng-web-apis/mutation-observer'
import { tuiInjectElement, tuiIsElement } from '@taiga-ui/cdk'

/** Cross-origin links open in new tabs. Link clicks do not bubble beyond the host. */
@Directive({
  selector: '[safeLinks]',
  host: { '(click)': 'onClick($event)' },
  providers: [
    WaMutationObserverService,
    provideMutationObserverInit({ childList: true, subtree: true }),
  ],
})
export class SafeLinksDirective {
  private readonly doc = inject(DOCUMENT)
  private readonly el = tuiInjectElement()
  private readonly sub = inject(WaMutationObserverService)
    .pipe(takeUntilDestroyed())
    .subscribe(() => {
      Array.from(this.doc.links)
        .filter(
          link =>
            link.origin !== this.doc.location.origin && this.el.contains(link),
        )
        .forEach(link => {
          link.target = '_blank'
          link.setAttribute('rel', 'noopener noreferrer')
          link.classList.add('g-external-link')
        })
    })

  protected onClick(event: MouseEvent) {
    if (
      tuiIsElement(event.target) &&
      this.el.contains(event.target.closest('a[href]'))
    ) {
      event.stopPropagation()
    }
  }
}
