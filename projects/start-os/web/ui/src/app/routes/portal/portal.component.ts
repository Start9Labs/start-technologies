import { Component, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { RouterOutlet } from '@angular/router'
import { WA_IS_MOBILE } from '@ng-web-apis/platform'
import { i18nPipe, LeafProgressPipe } from '@start9labs/shared'
import {
  TuiButton,
  TuiCell,
  TuiIcon,
  TuiLoader,
  TuiPopup,
  TuiScrollbar,
} from '@taiga-ui/core'
import { TuiActionBar, TuiProgress } from '@taiga-ui/kit'
import { PatchDB } from 'patch-db-client'
import { TabsComponent } from 'src/app/routes/portal/components/tabs.component'
import { OSService } from 'src/app/services/os.service'
import { DataModel } from 'src/app/services/patch-db/data-model'
import { PluginsService } from 'src/app/services/plugins.service'
import { PowerService } from 'src/app/services/power.service'
import { HeaderComponent } from './components/header/header.component'

@Component({
  template: `
    <!-- <header appHeader><app-plugins /></header> -->
    <header appHeader></header>
    <main>
      <tui-scrollbar [style.max-height.%]="100">
        <router-outlet />
      </tui-scrollbar>
    </main>
    <app-tabs />
    @if (deferredPower(); as action) {
      <tui-action-bar *tuiPopup="true">
        <span tuiCell="m">
          <tui-icon icon="@tui.power" />
          @if (action === 'restart') {
            {{
              'A backup is running. Your server will restart when it finishes.'
                | i18n
            }}
          } @else {
            {{
              'A backup is running. Your server will shut down when it finishes.'
                | i18n
            }}
          }
        </span>
        <button
          tuiButton
          size="s"
          appearance="secondary"
          (click)="cancelDeferredPower()"
        >
          {{ 'Cancel' | i18n }}
        </button>
      </tui-action-bar>
    } @else if (update(); as update) {
      <tui-action-bar *tuiPopup="bar()">
        <span tuiCell="m">
          @let leaf = update.overall | leafProgress;
          @if (leaf && leaf !== true && leaf.total) {
            <tui-progress-circle
              size="xxs"
              [style.display]="'flex'"
              [max]="100"
              [value]="getProgress(leaf.total, leaf.done)"
            />
            Downloading:
            {{ getProgress(leaf.total, leaf.done) }}%
          } @else {
            <tui-loader />
            Calculating download size
          }
        </span>
      </tui-action-bar>
    } @else if (restartReason(); as reason) {
      <tui-action-bar *tuiPopup="bar()">
        <span tuiCell="m">
          <tui-icon icon="@tui.refresh-cw" />
          @switch (reason) {
            @case ('update') {
              {{ 'Download complete. Restart to apply.' | i18n }}
            }
            @case ('mdns') {
              {{
                'Hostname changed, restart for installed services to use the new address'
                  | i18n
              }}
            }
            @case ('language') {
              {{
                'Language changed, restart for installed services to use the new language'
                  | i18n
              }}
            }
            @case ('kiosk') {
              {{ 'Kiosk mode changed, restart to apply' | i18n }}
            }
          }
        </span>
        <button tuiButton size="s" appearance="primary" (click)="restart()">
          {{ 'Restart' | i18n }}
        </button>
      </tui-action-bar>
    }
  `,
  styles: `
    @use '@taiga-ui/styles/utils' as taiga;

    @keyframes open {
      from {
        inline-size: 100%;
      }

      to {
        inline-size: calc(320px + (100% - 640px) * var(--plugins));
      }
    }

    :host {
      @include taiga.transition(inline-size);

      block-size: 100%;
      inline-size: 100%;
      display: flex;
      flex-direction: column;

      &._plugins {
        inline-size: calc(320px + (100% - 640px) * var(--plugins));
        animation: open var(--tui-duration) ease-in-out;
        transition: none;

        app-tabs {
          inline-size: calc(100% - var(--bumper));
        }
      }

      &::before,
      &::after {
        content: '';
        position: fixed;
        inset: 0;
        backdrop-filter: blur(0.5rem);
      }

      &::after {
        z-index: -1;
        // @TODO Theme
        background: url(/assets/img/background_dark.jpeg) fixed center/cover;
      }
    }

    main {
      flex: 1;
      overflow: hidden;
      margin: 0 var(--bumper) var(--bumper);
      filter: grayscale(1) brightness(0.75);

      @include taiga.transition(filter);

      header:has([data-status='success']) + & {
        filter: none;
      }
    }

    [tuiCell] {
      padding: 0;
      white-space: normal;
      text-wrap: balance;
    }
  `,
  host: {
    '[class._plugins]': '!mobile && plugins.enabled()',
    '[style.--plugins]': 'plugins.size() / 100',
  },
  imports: [
    RouterOutlet,
    HeaderComponent,
    TabsComponent,
    TuiScrollbar,
    TuiActionBar,
    TuiProgress,
    TuiLoader,
    TuiIcon,
    TuiButton,
    TuiPopup,
    TuiCell,
    i18nPipe,
    LeafProgressPipe,
  ],
})
export class PortalComponent {
  private readonly patch = inject<PatchDB<DataModel>>(PatchDB)
  private readonly power = inject(PowerService)

  readonly mobile = inject(WA_IS_MOBILE)
  readonly plugins = inject(PluginsService)
  readonly update = toSignal(inject(OSService).updating$)
  readonly restartReason = toSignal(
    this.patch.watch$('serverInfo', 'statusInfo', 'restart'),
  )
  readonly deferredPower = toSignal(
    this.patch.watch$('serverInfo', 'statusInfo', 'deferredPowerAction'),
  )
  readonly bar = signal(true)

  getProgress(size: number, downloaded: number): number {
    return Math.round((100 * downloaded) / (size || 1))
  }

  restart() {
    // Only stop offering the restart once one is actually under way — a
    // deferred or dismissed one leaves the reason for this bar in place.
    this.power.power('restart').subscribe(deferred => {
      if (!deferred) this.bar.set(false)
    })
  }

  cancelDeferredPower() {
    this.power.cancel()
  }
}
