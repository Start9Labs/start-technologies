import { Component, input } from '@angular/core'
import { TuiTitle } from '@taiga-ui/core'
import { TuiProgress } from '@taiga-ui/kit'
import { TuiCardLarge, TuiHeader } from '@taiga-ui/layout'
import { LogsWindowComponent } from './logs-window.component'
import { i18nPipe } from '../../i18n/i18n.pipe'

@Component({
  selector: 'app-initializing',
  template: `
    <section tuiCardLarge>
      <header tuiHeader>
        <h1 tuiTitle>
          {{
            initialSetup()
              ? ('Setting up your server' | i18n)
              : ('Booting StartOS' | i18n)
          }}
        </h1>
      </header>
      <div>
        {{ 'Progress' | i18n }}: {{ (progress().total * 100).toFixed(0) }}%
      </div>
      <progress tuiProgressBar [value]="progress().total" max="1"></progress>
      <p [innerHTML]="progress().message || '...'"></p>
    </section>
    <logs-window />
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      block-size: 100%;
    }

    section {
      margin: 1.5rem;
    }

    logs-window {
      flex: 1;
      padding: 1rem;
      margin: 0 1.5rem auto;
      overflow: hidden;
    }
  `,
  imports: [
    LogsWindowComponent,
    TuiProgress,
    TuiCardLarge,
    TuiHeader,
    TuiTitle,
    i18nPipe,
  ],
})
export class InitializingComponent {
  /** Progress is observational; the owning operation establishes completion. */
  readonly progress = input<{ total: number; message: string }>({
    total: 0,
    message: '',
  })
  readonly initialSetup = input(false)
}
