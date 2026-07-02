import { Component, input } from '@angular/core'
import { TuiIcon } from '@taiga-ui/core'

@Component({
  selector: 'app-placeholder',
  template: `
    <tui-icon [style.font-size.rem]="2" [icon]="icon()" />
    <ng-content />
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      place-items: center;
      gap: 0.5rem;
      color: var(--tui-text-tertiary);
      padding: 0.5rem;
    }
  `,
  imports: [TuiIcon],
})
export class Placeholder {
  public readonly icon = input('@tui.search')
}
