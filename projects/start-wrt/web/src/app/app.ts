import {
  Component,
  computed,
  effect,
  ElementRef,
  inject,
  linkedSignal,
  viewChild,
} from '@angular/core'
import { takeUntilDestroyed, toSignal } from '@angular/core/rxjs-interop'
import { NavigationEnd, Router, RouterOutlet } from '@angular/router'
import { TUI_BREAKPOINT, TUI_DARK_MODE, TuiScrollbar } from '@taiga-ui/core'
import { filter } from 'rxjs'
import { TuiNavigation } from '@taiga-ui/layout'
import { Aside } from 'src/app/components/aside'
import { Header } from 'src/app/components/header'
import { Nav } from 'src/app/components/nav'
import { StaleUiAlert } from 'src/app/components/stale-ui-alert'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { i18nService } from 'src/app/i18n/i18n.service'
import { SystemService } from 'src/app/services/system.service'
import { Language } from 'src/app/utils/languages'

@Component({
  selector: 'app-outlet',
  template: `
    <header tuiNavigationHeader [attr.tuiTheme]="dark() ? 'dark' : null">
      <app-header />
    </header>
    <aside
      class="_expanded"
      [class._expanded]="open()"
      [tuiNavigationAside]="open()"
      [attr.tuiTheme]="dark() ? 'dark' : null"
    >
      <nav appNav></nav>
      <footer>
        <button
          tuiAsideItem
          type="button"
          [iconStart]="open() ? '@tui.chevron-left' : '@tui.chevron-right'"
          (click)="open.set(!open())"
        >
          {{ (open() ? 'Collapse' : 'Expand') | i18n }}
        </button>
      </footer>
    </aside>
    <main>
      <tui-scrollbar><router-outlet /></tui-scrollbar>
    </main>
    <aside appAside inert></aside>
    <stale-ui-alert />
  `,
  styles: `
    :host {
      height: 100%;
      display: grid;
      grid-template: 3rem 1fr / min-content 1fr 20.75rem;
      overflow: hidden;
      transition: grid-template var(--tui-duration);

      &:has(aside[inert]) {
        grid-template: 3rem 1fr / min-content 1fr 0;
      }

      header {
        grid-column: span 3;

        &::before {
          clip-path: inset(0 0 0 2rem);
        }
      }
    }

    main {
      isolation: isolate;
      overflow: hidden;
    }

    tui-scrollbar {
      height: 100%;
      min-width: 100%;

      ::ng-deep > .t-content {
        height: 100%;
      }
    }

    router-outlet + ::ng-deep ng-component {
      display: flex;
      flex-direction: column;
      margin: 1rem 1.5rem;

      > header[tuiHeader] {
        position: sticky;
        inset-inline-start: 1.5rem;
        max-width: 100%;

        + tui-tabs {
          flex-shrink: 0;
          margin-block: 1rem;
        }
      }

      &::after {
        content: '';
        block-size: 5rem;
        flex-shrink: 0;
      }
    }

    // On mobile, pin the content to the viewport width so expanding the nav
    // slides content off-screen instead of reflowing/squishing it. Mirrors
    // start-os start-tunnel's outlet (desktop min: 100%, mobile pinned to the
    // collapsed-rail width). The pin goes on the page content, NOT on
    // tui-scrollbar: the scrollport must keep tracking the visible main
    // column, so that with the nav expanded the pinned content overflows it
    // and stays reachable by horizontal scroll — a pinned scrollport would
    // itself be clipped by main's overflow, and no scrollbar could appear.
    // 6rem = the collapsed 3rem rail + the content's own 1.5rem side margins.
    :host-context(tui-root._mobile) router-outlet + ::ng-deep ng-component {
      min-width: calc(100vw - 6rem);
    }

    :host-context(tui-root._mobile)
      router-outlet
      + ::ng-deep
      ng-component
      > header[tuiHeader] {
      max-width: calc(100vw - 3rem);
    }
  `,
  imports: [
    Header,
    Nav,
    Aside,
    RouterOutlet,
    StaleUiAlert,
    TuiScrollbar,
    TuiNavigation,
    i18nPipe,
  ],
})
export class App {
  protected readonly dark = inject(TUI_DARK_MODE)
  private readonly router = inject(Router)
  private readonly breakpoint = inject(TUI_BREAKPOINT)
  private readonly navigation = toSignal(
    this.router.events.pipe(filter(e => e instanceof NavigationEnd)),
  )
  // Collapse the nav whenever Taiga's mobile breakpoint activates (the same
  // signal that flips tui-root._mobile): mobile mode pins the content to
  // 100vw - 3rem — the width the collapsed rail leaves — so an expanded
  // in-flow aside would push that pinned content under the shell's overflow
  // clip with no way to scroll to it. Mirrors start-tunnel's outlet;
  // re-evaluating on navigation re-collapses a nav expanded by hand on mobile.
  protected readonly open = linkedSignal<string[], boolean>({
    source: () => [this.breakpoint(), String(this.navigation())],
    computation: (source, previous) =>
      previous?.value !== false && source[0] !== 'mobile',
  })
  protected readonly scrollbar = viewChild(TuiScrollbar, { read: ElementRef })
  protected readonly _ = this.router.events
    .pipe(takeUntilDestroyed())
    .subscribe(() => {
      this.scrollbar()?.nativeElement.scrollTo({ top: 0 })
    })

  constructor() {
    const system = inject(SystemService)
    const i18n = inject(i18nService)
    system.init()
    // Saved settings are the source of truth: apply theme and language
    // globally when they load or change (boot + after Save). Keyed on the
    // values, not the info object — a refresh that changes neither (e.g. the
    // 30s stale-UI heartbeat) must not re-apply saved settings over an
    // unsaved live preview on Settings → General.
    const language = computed(() => system.info()?.language)
    const theme = computed(() => system.info()?.theme)
    effect(() => {
      const lang = language()
      if (lang) i18n.setLangLocal(lang as Language)
    })
    effect(() => {
      const t = theme()
      if (!t) return
      if (t === 'system') {
        this.dark.reset()
      } else {
        this.dark.set(t === 'dark')
      }
    })
  }
}
