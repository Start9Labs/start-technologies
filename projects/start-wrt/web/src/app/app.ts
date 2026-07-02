import {
  Component,
  effect,
  ElementRef,
  inject,
  signal,
  viewChild,
} from '@angular/core'
import { takeUntilDestroyed } from '@angular/core/rxjs-interop'
import { Router, RouterOutlet } from '@angular/router'
import { WA_IS_MOBILE } from '@ng-web-apis/platform'
import { TUI_DARK_MODE, TuiScrollbar } from '@taiga-ui/core'
import { TuiNavigation } from '@taiga-ui/layout'
import { Aside } from 'src/app/components/aside'
import { Header } from 'src/app/components/header'
import { Nav } from 'src/app/components/nav'
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
    // start-os start-tunnel's outlet (desktop min: 100%, mobile: 100vw - 3rem).
    :host-context(tui-root._mobile) tui-scrollbar {
      min-width: calc(100vw - 3rem);
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
    TuiScrollbar,
    TuiNavigation,
    i18nPipe,
  ],
})
export class App {
  protected readonly dark = inject(TUI_DARK_MODE)
  protected readonly open = signal(!inject(WA_IS_MOBILE))
  protected readonly scrollbar = viewChild(TuiScrollbar, { read: ElementRef })
  protected readonly _ = inject(Router)
    .events.pipe(takeUntilDestroyed())
    .subscribe(() => {
      this.scrollbar()?.nativeElement.scrollTo({ top: 0 })
    })

  constructor() {
    const system = inject(SystemService)
    const i18n = inject(i18nService)
    system.init()
    effect(() => {
      const info = system.info()
      if (!info) return
      // Saved settings are the source of truth: apply theme and language
      // globally whenever system info loads or changes (boot + after Save).
      i18n.setLangLocal(info.language as Language)
      if (info.theme === 'system') {
        this.dark.reset()
      } else {
        this.dark.set(info.theme === 'dark')
      }
    })
  }
}
