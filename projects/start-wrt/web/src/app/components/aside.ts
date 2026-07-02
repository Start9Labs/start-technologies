import { Component, computed, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { NavigationEnd, Router } from '@angular/router'
import { TuiScrollbar } from '@taiga-ui/core'
import { filter, map } from 'rxjs'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { HELP_OPEN, HelpService } from 'src/app/help/help'
import { MarkdownPipe } from '@start9labs/shared'

@Component({
  selector: '[appAside]',
  template: `
    <tui-scrollbar>
      <div class="g-help" [innerHTML]="help() | markdown | dompurify"></div>
    </tui-scrollbar>
  `,
  styles: `
    :host {
      position: relative;
      width: 20.75rem;
      background: var(--tui-background-base);
      box-shadow:
        inset 0.25rem 0 var(--tui-theme-color),
        0 -0.25rem var(--tui-theme-color);
      border-top-left-radius: var(--tui-radius-xs);
      transition: transform var(--tui-duration);

      &::before {
        content: '';
        position: absolute;
        inset-block-start: 0;
        inset-inline-end: 100%;
        block-size: 1rem;
        inline-size: 1rem;
        pointer-events: none;
        border-top-right-radius: var(--tui-radius-xs);
        box-shadow: 0 -0.25rem var(--tui-theme-color);
      }

      &[inert] {
        transform: translate3d(100%, 0, 0);
      }
    }

    :host-context(tui-root._mobile) {
      position: absolute;
      z-index: 1;
      top: 3rem;
      bottom: 0;
      inset-inline-end: 0;
      animation: tuiFade 0s 1s both;
    }
  `,
  host: { '[attr.inert]': '!open() || null' },
  imports: [TuiScrollbar, MarkdownPipe, NgDompurifyPipe],
})
export class Aside {
  protected readonly open = inject(HELP_OPEN)
  private readonly helpService = inject(HelpService)
  private readonly router = inject(Router)
  private readonly url = toSignal(
    this.router.events.pipe(
      filter(e => e instanceof NavigationEnd),
      map(({ urlAfterRedirects }) => urlAfterRedirects.split('?')[0]),
    ),
    { initialValue: this.router.url.split('?')[0] },
  )
  protected readonly help = computed(
    () => this.helpService.content()[this.url()],
  )
}
