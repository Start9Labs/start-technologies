import { KeyValuePipe } from '@angular/common'
import {
  Component,
  Input,
  signal,
  SkipSelf,
  ViewEncapsulation,
} from '@angular/core'
import { ControlContainer, ReactiveFormsModule } from '@angular/forms'
import { IST } from '@start9labs/start-core'
import { TUI_DEFAULT_ERROR_MESSAGE } from '@taiga-ui/core'
import { identity } from 'rxjs'

import { FilterHiddenPipe } from '../pipes/filter-hidden.pipe'
import { FormArrayComponent } from './array.component'
import { FormControlComponent } from './control.component'
import { FormObjectComponent } from './object.component'
import { FormUnionComponent } from './union.component'

@Component({
  selector: 'form-group',
  template: `
    @for (entry of spec | keyvalue: asIsOrder | filterHidden; track entry) {
      @switch (entry.value.type) {
        @case ('object') {
          <form-object
            class="g-form-control"
            [formGroupName]="entry.key"
            [spec]="$any(entry.value)"
          />
        }
        @case ('union') {
          <form-union
            class="g-form-control"
            [formGroupName]="entry.key"
            [spec]="$any(entry.value)"
          />
        }
        @case ('list') {
          <form-array [formArrayName]="entry.key" [spec]="$any(entry.value)" />
        }
        @default {
          <form-control
            class="g-form-control"
            [formControlName]="entry.key"
            [spec]="entry.value"
          />
        }
      }
    }
  `,
  styles: `
    form-group > form-array:first-child {
      margin-block-start: 0;
    }

    form-group .g-form-control:not(:first-child) {
      display: block;
      margin-top: 1rem;
    }

    form-group .g-form-group {
      position: relative;
      padding-left: var(--tui-height-m);

      &::before,
      &::after {
        content: '';
        position: absolute;
        background: var(--tui-background-neutral-1);
      }

      &::before {
        top: 0;
        left: calc(1rem - 1px);
        bottom: 0.5rem;
        width: 2px;
      }

      &::after {
        left: 0.75rem;
        bottom: 0;
        width: 0.5rem;
        height: 0.5rem;
        border-radius: 100%;
      }
    }

    form-group tui-tooltip {
      z-index: 1;
      margin-left: 0.25rem;
    }
  `,
  encapsulation: ViewEncapsulation.None,
  viewProviders: [
    {
      provide: TUI_DEFAULT_ERROR_MESSAGE,
      useValue: signal('Unknown error'),
    },
    {
      provide: ControlContainer,
      deps: [[new SkipSelf(), ControlContainer]],
      useFactory: identity,
    },
  ],
  imports: [
    KeyValuePipe,
    ReactiveFormsModule,
    FilterHiddenPipe,
    FormControlComponent,
    FormObjectComponent,
    FormArrayComponent,
    FormUnionComponent,
  ],
})
export class FormGroupComponent {
  @Input() spec: IST.InputSpec = {}

  asIsOrder() {
    return 0
  }
}
