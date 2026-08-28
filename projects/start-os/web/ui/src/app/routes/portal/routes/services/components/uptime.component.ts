import { AsyncPipe } from '@angular/common'
import { Component, input } from '@angular/core'
import { i18nPipe } from '@start9labs/shared'
import { map, timer } from 'rxjs'
import { distinctUntilChanged } from 'rxjs/operators'

@Component({
  selector: 'service-uptime',
  template: `
    <header>{{ 'Uptime' | i18n }}</header>
    <section>
      @if (uptime$ | async; as time) {
        <div>
          <label>{{ time.days }}</label>
          <span>{{ 'Days' | i18n }}</span>
        </div>
        <div>
          <label>{{ time.hours }}</label>
          <span>{{ 'Hours' | i18n }}</span>
        </div>
        <div>
          <label>{{ time.minutes }}</label>
          <span>{{ 'Minutes' | i18n }}</span>
        </div>
        <div>
          <label>{{ time.seconds }}</label>
          <span>{{ 'Seconds' | i18n }}</span>
        </div>
      }
    </section>
  `,
  styles: [
    `
      :host {
        grid-column: span 3;
      }

      h3 {
        font: var(--tui-typography-heading-h4);
        font-weight: normal;
        margin: 0;
        text-align: center;
      }

      section {
        height: 100%;
        max-width: 100%;
        display: grid;
        grid-template-columns: repeat(4, 1fr);
        gap: 1rem;
        place-content: center;
        margin: auto;
        padding: 1rem 0;
        text-align: center;
        text-transform: uppercase;
        color: var(--tui-text-secondary);
        font: var(--tui-typography-ui-2xs);
        font-size: min(4cqw, 0.75rem);
      }

      label {
        display: block;
        font-size: min(6vw, 2.5rem);
        font-size: min(20cqw, 2.5rem);
        margin: 1rem 0;
        color: var(--tui-text-primary);
      }

      :host-context(table),
      :host-context(service-status),
      :host-context(.service-tile) {
        padding: 0;

        header {
          display: none;
        }

        section {
          display: flex;
          justify-content: flex-start;
          gap: 0.5rem;
          padding: 0;
          line-height: 1;
        }

        label {
          margin: 0;
          font-size: 1rem;
        }

        div {
          display: flex;
          align-items: baseline;
        }

        span {
          font-size: 0;

          &:first-letter {
            font-size: 0.75rem;
          }
        }
      }
    `,
  ],
  imports: [i18nPipe, AsyncPipe],
})
export class ServiceUptimeComponent {
  protected readonly uptime$ = timer(0, 1000).pipe(
    map(() => {
      const started = this.started()
      return started ? Math.max(Date.now() - new Date(started).getTime(), 0) : 0
    }),
    distinctUntilChanged(),
    map(delta => ({
      seconds: Math.floor(delta / 1000) % 60,
      minutes: Math.floor(delta / (1000 * 60)) % 60,
      hours: Math.floor(delta / (1000 * 60 * 60)) % 24,
      days: Math.floor(delta / (1000 * 60 * 60 * 24)),
    })),
  )

  readonly started = input<string | null>(null)
}
