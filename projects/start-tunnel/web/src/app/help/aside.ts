import { Component, computed, inject } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { NavigationEnd, Router } from '@angular/router'
import { MarkdownPipe } from '@start9labs/shared'
import { TuiScrollbar } from '@taiga-ui/core'
import { NgDompurifyPipe } from '@taiga-ui/dompurify'
import { filter, map } from 'rxjs'
import { HELP_OPEN, HelpService } from 'src/app/help/help'

/**
 * The persistent contextual help sidebar. Keyed by the current router URL, so
 * each top-level screen (`/subnets`, `/devices`, …) shows its own help. Toggled
 * by the Help switch in the header (`HELP_OPEN`). Dialogs, which don't change
 * the URL, use `modal-help.ts` instead. Collapses to zero width when closed so
 * the main content reclaims the space.
 */
@Component({
  selector: '[appAside]',
  template: `
    <tui-scrollbar>
      <div class="g-help" [innerHTML]="help() | markdown | dompurify"></div>
    </tui-scrollbar>
  `,
  styles: `
    :host {
      flex: 0 0 auto;
      inline-size: 20.75rem;
      overflow: hidden;
      background: var(--tui-background-base);
      box-shadow:
        inset 0.25rem 0 var(--tui-theme-color),
        0 -0.25rem var(--tui-theme-color);
      border-start-start-radius: var(--tui-radius-s);
      transition: inline-size var(--tui-duration);

      &[inert] {
        inline-size: 0;
      }
    }

    // Fixed inner width so the content doesn't reflow while the host collapses.
    tui-scrollbar {
      block-size: 100%;
      inline-size: 20.75rem;
    }

    // On mobile, overlay the content (slide in from the right) instead of
    // reflowing it, matching the nav aside's behavior.
    :host-context(tui-root._mobile) {
      position: absolute;
      z-index: 1;
      inset-block: 0;
      inset-inline-end: 0;
      inline-size: min(20.75rem, calc(100vw - 3rem));
      transition: transform var(--tui-duration);

      &[inert] {
        inline-size: min(20.75rem, calc(100vw - 3rem));
        transform: translate3d(100%, 0, 0);
      }
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
      map(({ urlAfterRedirects }) => urlAfterRedirects.split('?')[0] ?? ''),
    ),
    { initialValue: this.router.url.split('?')[0] ?? '' },
  )
  protected readonly help = computed(
    () => this.helpService.content()[this.url()] ?? '',
  )
}
