import { booleanAttribute, Directive, input } from '@angular/core'

@Directive({
  selector: 'tui-badge-notification[appUpdatesCount]',
  host: {
    '[style.background]':
      'appUpdatesCount() ? (security() ? "var(--tui-status-negative)" : "var(--tui-status-positive)") : null',
  },
})
export class UpdatesCount {
  public readonly appUpdatesCount = input(true, { transform: booleanAttribute })
  public readonly security = input(false)
}
