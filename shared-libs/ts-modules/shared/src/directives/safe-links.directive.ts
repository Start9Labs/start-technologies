import { Directive, DOCUMENT, inject } from '@angular/core'
import { takeUntilDestroyed } from '@angular/core/rxjs-interop'
import {
  provideMutationObserverInit,
  WaMutationObserverService,
} from '@ng-web-apis/mutation-observer'
import { tuiInjectElement } from '@taiga-ui/cdk'

@Directive({
  selector: '[safeLinks]',
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
            link.hostname !== this.doc.location.hostname &&
            this.el.contains(link),
        )
        .forEach(link => {
          link.target = '_blank'
          link.setAttribute('rel', 'noopener noreferrer')
          link.classList.add('g-external-link')
        })
    })
}
