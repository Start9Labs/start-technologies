import { Component } from '@angular/core'
import { i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiDialogContext, TuiTitle } from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
@Component({
  template: `
    <header tuiHeader>
      <h2 tuiTitle [id]="context.id">
        {{ (context.data === 'restart' ? 'Restart' : 'Warning') | i18n }}
      </h2>
    </header>
    <p>
      {{
        'A backup is currently running. Interrupting it now can corrupt the backup being written.'
          | i18n
      }}
    </p>
    @if (context.data === 'shutdown') {
      <p>
        {{
          'Are you sure you want to power down your server? This can take several minutes, and your server will not come back online automatically. To power on again, You will need to physically unplug your server and plug it back in.'
            | i18n
        }}
      </p>
    }
    <footer class="g-buttons">
      <button
        tuiButton
        appearance="secondary"
        (click)="context.completeWith(true)"
      >
        {{
          (context.data === 'restart' ? 'Restart now' : 'Shut down now') | i18n
        }}
      </button>
      <button tuiButton (click)="context.completeWith(false)">
        {{ 'Wait for backup to complete' | i18n }}
      </button>
    </footer>
  `,
  styles: `
    button {
      white-space: normal;
      block-size: auto;
      min-block-size: var(--tui-height-m);
    }
  `,
  imports: [TuiButton, TuiHeader, TuiTitle, i18nPipe],
})
export class PowerComponent {
  protected readonly context =
    injectContext<TuiDialogContext<boolean, T.PowerAction>>()
}

export const POWER = new PolymorpheusComponent(PowerComponent)
