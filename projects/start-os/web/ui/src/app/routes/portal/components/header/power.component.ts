import { Component } from '@angular/core'
import { takeUntilDestroyed, toSignal } from '@angular/core/rxjs-interop'
import { i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiDialogContext } from '@taiga-ui/core'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { map, take, timer } from 'rxjs'

// Long enough to read the warning, short enough to not strand someone who
// walked away mid-decision.
const COUNTDOWN = 30

@Component({
  template: `
    <p>
      {{
        'A backup is currently running. Powering down now can corrupt the backup of the service being written.'
          | i18n
      }}
    </p>
    <footer class="g-buttons">
      @if (action === 'restart') {
        <button tuiButton appearance="secondary" (click)="now()">
          {{ 'Restart now' | i18n }}
        </button>
      } @else {
        <button tuiButton appearance="secondary" (click)="now()">
          {{ 'Shut down now' | i18n }}
        </button>
      }
      <button tuiButton (click)="wait()">
        {{ 'Wait for backup to complete' | i18n }} ({{ seconds() }})
      </button>
    </footer>
  `,
  imports: [TuiButton, i18nPipe],
})
export class PowerComponent {
  private readonly context =
    injectContext<TuiDialogContext<boolean, T.PowerAction>>()

  protected readonly action = this.context.data

  protected readonly seconds = toSignal(
    timer(0, 1000).pipe(
      map(tick => COUNTDOWN - tick),
      take(COUNTDOWN + 1),
    ),
    { initialValue: COUNTDOWN },
  )

  constructor() {
    // Choosing neither is choosing to wait.
    timer(COUNTDOWN * 1000)
      .pipe(takeUntilDestroyed())
      .subscribe(() => this.wait())
  }

  protected now() {
    this.context.completeWith(true)
  }

  protected wait() {
    this.context.completeWith(false)
  }
}

export const POWER = new PolymorpheusComponent(PowerComponent)
