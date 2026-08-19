import { Component, signal } from '@angular/core'
import { takeUntilDestroyed } from '@angular/core/rxjs-interop'
import { i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiDialogContext } from '@taiga-ui/core'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { take, timer } from 'rxjs'

const COUNTDOWN = 30

@Component({
  template: `
    <p>
      {{
        'A backup is currently running. Interrupting it now can corrupt the backup of the service being written.'
          | i18n
      }}
    </p>
    @if (action === 'shutdown') {
      <p>
        {{
          'Are you sure you want to power down your server? This can take several minutes, and your server will not come back online automatically. To power on again, You will need to physically unplug your server and plug it back in.'
            | i18n
        }}
      </p>
    }
    <footer class="g-buttons">
      <button tuiButton appearance="secondary" (click)="now()">
        {{ (action === 'restart' ? 'Restart now' : 'Shut down now') | i18n }}
      </button>
      <button tuiButton (click)="wait()">
        {{ 'Wait for backup to complete' | i18n }} ({{ seconds() }})
      </button>
    </footer>
  `,
  // Both labels are sentences, and neither shrinks: 25rem cannot hold them on
  // one row in any locale.
  styles: 'footer { flex-wrap: wrap }',
  imports: [TuiButton, i18nPipe],
})
export class PowerComponent {
  private readonly context =
    injectContext<TuiDialogContext<boolean, T.PowerAction>>()

  protected readonly action = this.context.data
  protected readonly seconds = signal(COUNTDOWN)

  constructor() {
    // One timer, so the choice is made exactly when the label says it will be.
    timer(0, 1000)
      .pipe(take(COUNTDOWN + 1), takeUntilDestroyed())
      .subscribe(tick => {
        this.seconds.set(COUNTDOWN - tick)
        if (tick === COUNTDOWN) this.wait()
      })
  }

  protected now() {
    this.context.completeWith(true)
  }

  protected wait() {
    this.context.completeWith(false)
  }
}

export const POWER = new PolymorpheusComponent(PowerComponent)
