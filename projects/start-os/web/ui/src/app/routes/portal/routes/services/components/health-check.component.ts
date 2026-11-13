import { Component, computed, inject, input } from '@angular/core'
import { i18nKey, i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiIcon, TuiLoader } from '@taiga-ui/core'

@Component({
  selector: 'tr[healthCheck]',
  template: `
    <td class="name">{{ healthCheck().name }}</td>
    <td>
      <span>
        @if (loading()) {
          <tui-loader size="m" />
        } @else {
          <tui-icon [icon]="icon()" [style.color]="color()" />
        }
        {{ message() }}
      </span>
    </td>
  `,
  styles: `
    span {
      display: flex;
      align-items: start;
      gap: 0.5rem;
    }

    tui-loader,
    tui-icon {
      flex-shrink: 0;
    }

    .name {
      width: 9.5rem;
    }

    :host-context(tui-root._mobile) {
      display: flex;
      flex-direction: column;

      td {
        width: 100%;

        &:first-child {
          font-weight: bold;
          padding-bottom: 0;
        }

        &:last-child {
          color: var(--tui-text-secondary);
        }
      }
    }
  `,
  imports: [TuiLoader, TuiIcon],
})
export class ServiceHealthCheckComponent {
  private readonly i18n = inject(i18nPipe)

  public readonly healthCheck = input.required<T.NamedHealthCheckResult>()

  protected readonly loading = computed(
    (health = this.healthCheck()) =>
      (health.result === 'starting' || health.result === 'loading') &&
      !health.waiting,
  )

  protected readonly icon = computed((health = this.healthCheck()) => {
    if ('waiting' in health && health.waiting) return '@tui.clock'

    switch (health.result) {
      case 'success':
        return '@tui.check'
      case 'failure':
        return '@tui.triangle-alert'
      default:
        return '@tui.minus'
    }
  })

  protected readonly color = computed((health = this.healthCheck()) => {
    if ('waiting' in health && health.waiting)
      return 'var(--tui-text-secondary)'

    switch (health.result) {
      case 'success':
        return 'var(--tui-status-positive)'
      case 'failure':
        return 'var(--tui-status-warning)'
      case 'starting':
      case 'loading':
        return 'var(--tui-background-accent-1)'
      default:
        return 'var(--tui-text-secondary)'
    }
  })

  protected readonly message = computed(
    ({ result, message } = this.healthCheck()) =>
      message ||
      this.i18n.transform(
        (
          {
            starting: 'Starting',
            success: 'Success',
            loading: 'Loading',
            failure: 'Failure',
            disabled: 'Disabled',
          } satisfies Record<T.HealthStatus, i18nKey>
        )[result],
      ),
  )
}
