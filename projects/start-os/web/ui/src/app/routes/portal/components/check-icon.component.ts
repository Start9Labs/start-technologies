import { Component, input } from '@angular/core'
import { TuiIcon, TuiLoader } from '@taiga-ui/core'

@Component({
  selector: 'check-icon',
  template: `
    @if (loading()) {
      <tui-loader size="s" />
    } @else if (pass() === true) {
      <tui-icon class="g-positive" icon="@tui.check" />
    } @else if (pass() === false) {
      <tui-icon class="g-negative" icon="@tui.x" />
    } @else {
      <tui-icon class="g-secondary" icon="@tui.minus" />
    }
  `,
  styles: `
    tui-icon {
      font-size: 1.3rem;
      vertical-align: text-bottom;
    }
  `,
  imports: [TuiIcon, TuiLoader],
})
export class CheckIconComponent {
  readonly pass = input<boolean | undefined>(undefined)
  readonly loading = input(false)
}
