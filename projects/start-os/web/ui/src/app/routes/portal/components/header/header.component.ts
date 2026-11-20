import {
  Component,
  inject,
  OnInit,
  viewChild,
  ViewContainerRef,
} from '@angular/core'

import { TitleService } from 'src/app/services/title.service'
import { HeaderMenuComponent } from './menu.component'
import { HeaderNavigationComponent } from './navigation.component'
import { HeaderStatusComponent } from './status.component'

@Component({
  selector: 'header[appHeader]',
  template: `
    <header-navigation />
    <div class="item item_center">
      <ng-content />
      <div class="mobile"><ng-container #vcr /></div>
    </div>
    <header-status class="item item_connection" />
    <header-menu class="item item_corner" />
  `,
  styles: `
    @use '@taiga-ui/styles/utils' as taiga;

    @keyframes connecting {
      25%,
      100% {
        background-position: 175%;
      }
    }

    :host {
      display: flex;
      height: 2.75rem;
      border-radius: var(--bumper);
      margin: var(--bumper);
      clip-path: inset(0 round var(--bumper));
      overflow: hidden;
      filter: grayscale(1) brightness(0.75);

      @include taiga.transition(filter);

      .mobile {
        display: none;
      }

      .item {
        position: relative;
        border-radius: inherit;
        isolation: isolate;

        &::before {
          @include taiga.transition(all);
          content: '';
          position: absolute;
          inset: 0;
          border-radius: inherit;
          backdrop-filter: blur(1rem);
          transform: skewX(30deg);
          background: color-mix(in hsl, var(--start9-base-2) 75%, transparent);
          box-shadow: inset 0 1px rgb(255 255 255 / 25%);
          z-index: -1;
        }

        &_center {
          flex: 1;
          min-width: 0;
        }

        &_connection::before {
          box-shadow:
            inset 0 1px rgba(255, 255, 255, 0.25),
            inset 0 -0.75rem 0 -0.5rem var(--status);
          background-image: linear-gradient(
            90deg,
            transparent 35%,
            rgba(255, 255, 255, 0.25),
            transparent 65%
          );
          background-size: 50% 100%;
          background-position: -75%;
          background-repeat: no-repeat;
          animation: connecting 2s 2s ease-in-out infinite;
        }

        &_corner {
          margin-inline-start: var(--bumper);

          &::before {
            right: -2rem;
          }
        }
      }

      &:has([data-status='error']) {
        --status: var(--tui-status-negative);
      }

      &:has([data-status='warning']) {
        --status: var(--tui-status-warning);
      }

      &:has([data-status='neutral']) {
        --status: var(--tui-status-neutral);
      }

      &:has([data-status='success']) {
        --status: transparent;
        // "none" breaks border radius in Firefox
        filter: grayscale(0.001);
      }
    }

    :host-context(tui-root._mobile) {
      .item_center::before {
        left: -2rem;
      }

      .mobile {
        display: flex;
        gap: 0.25rem;
        height: 100%;
        align-items: center;
        font: var(--tui-typography-body-l);
        padding: 1rem;
        white-space: nowrap;
        overflow: hidden;

        ::ng-deep > [tuiIconButton] {
          margin-inline-start: -1rem;
        }
      }
    }
  `,
  imports: [
    HeaderStatusComponent,
    HeaderNavigationComponent,
    HeaderMenuComponent,
  ],
})
export class HeaderComponent implements OnInit {
  private readonly title = inject(TitleService)

  readonly vcr = viewChild.required('vcr', { read: ViewContainerRef })

  ngOnInit() {
    this.title.register(this.vcr())
  }
}
