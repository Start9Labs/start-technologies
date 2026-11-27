import { Component, input, Pipe, PipeTransform } from '@angular/core'
import { i18nKey, i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiCell, TuiTitle } from '@taiga-ui/core'
import { TuiProgress } from '@taiga-ui/kit'
import { ValuePipe } from './value.pipe'

const NAMES: Record<T.SensorKind, i18nKey> = {
  cpu: 'Processor',
  gpu: 'Graphics',
  memory: 'Memory',
  storage: 'Storage drive',
  network: 'Network adapter',
  chipset: 'Chipset',
  system: 'System',
  other: 'Sensor',
}

/** Gauges start where no part is warm yet and fill at the sensor's critical limit. */
const FLOOR = 40

@Pipe({ name: 'gauge' })
export class GaugePipe implements PipeTransform {
  transform({ temperature, high, critical }: T.TemperatureSensor) {
    const floor = Math.min(FLOOR, Number.parseFloat(high.value) - 10)
    const t = Number.parseFloat(temperature.value) - floor
    const h = Number.parseFloat(high.value) - floor
    const c = Number.parseFloat(critical.value) - floor
    const warm = `${(100 * h) / c}%`

    return {
      max: c,
      value: Math.min(Math.max(t, 0), c),
      color: `linear-gradient(to right, var(--tui-status-positive) ${warm}, var(--tui-status-warning) ${warm}, var(--tui-status-negative))`,
      hot: t >= h && t < c,
      critical: t >= c,
    }
  }
}

@Component({
  selector: 'metrics-temperature',
  template: `
    @if (value(); as sensors) {
      @for (sensor of sensors; track sensor.id) {
        @let gauge = sensor | gauge;
        <div>
          <div tuiCell="s">
            <span tuiTitle>
              {{ names[sensor.kind] | i18n }}
              <span tuiSubtitle>{{ sensor.id }}</span>
            </span>
            <b
              [class.g-warning]="gauge.hot"
              [class.g-negative]="gauge.critical"
            >
              {{ sensor.temperature.value | value }}
              {{ sensor.temperature.unit }}
            </b>
          </div>
          <progress
            tuiProgressBar
            tuiProgressFixedGradient
            size="s"
            [max]="gauge.max"
            [value]="gauge.value"
            [color]="gauge.color"
          ></progress>
        </div>
      } @empty {
        <p class="g-secondary">
          {{ 'No temperature sensors found' | i18n }}
        </p>
      }
    }
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      gap: 0.75rem;
      padding-block-end: 0.625rem;
    }

    [tuiSubtitle] {
      color: var(--tui-text-tertiary);
    }
  `,
  imports: [TuiCell, TuiTitle, TuiProgress, GaugePipe, ValuePipe, i18nPipe],
})
export class TemperatureComponent {
  readonly value = input<readonly T.TemperatureSensor[]>()
  readonly names = NAMES
}
