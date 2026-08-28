import { Component, Input } from '@angular/core'
import { TuiAccordion, TuiFade } from '@taiga-ui/kit'
import { ActionSuccessMemberComponent } from './action-success-member.component'
import { ActionSuccessMultilineComponent } from './action-success-multiline.component'
import { GroupResult } from './types'

@Component({
  selector: 'app-action-success-group',
  template: `
    @for (member of group.value; track $index) {
      @if (member.type === 'single') {
        <app-action-success-member [member]="member" />
      }
      @if (member.type === 'multiline') {
        <app-action-success-multiline
          [multiline]="member"
          [name]="member.name"
          [description]="member.description || ''"
        />
      }
      @if (member.type === 'group') {
        <tui-accordion>
          <button tuiAccordion>
            <span tuiFade>{{ member.name }}</span>
          </button>
          <tui-expand>
            <app-action-success-group [group]="member" />
          </tui-expand>
        </tui-accordion>
      }
    }
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      gap: 1rem;
    }
  `,
  imports: [
    ActionSuccessMemberComponent,
    ActionSuccessMultilineComponent,
    TuiAccordion,
    TuiFade,
  ],
})
export class ActionSuccessGroupComponent {
  @Input()
  group!: GroupResult
}
