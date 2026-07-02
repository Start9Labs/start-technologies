import { Directive, inject } from '@angular/core'
import {
  ControlContainer,
  FormGroupDirective,
  FormGroupName,
} from '@angular/forms'
import { TuiSkeleton } from '@taiga-ui/kit'
import { TuiCardLarge, TuiForm } from '@taiga-ui/layout'

@Directive({
  selector: '[formLoading]',
  host: { class: 'g-form', style: 'overflow: visible' },
  hostDirectives: [
    TuiForm,
    TuiCardLarge,
    {
      directive: TuiSkeleton,
      inputs: ['tuiSkeleton: formLoading'],
    },
  ],
})
export class Form {}

export const FORM = {
  provide: ControlContainer,
  useFactory: () =>
    inject(FormGroupDirective, { self: true, optional: true }) ||
    inject(FormGroupName, { self: true, optional: true }),
}
