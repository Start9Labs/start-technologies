import { AsyncPipe } from '@angular/common'
import { Component, inject, input } from '@angular/core'
import { FormGroupDirective } from '@angular/forms'
import { TuiAnimated } from '@taiga-ui/cdk'
import { TuiButton, TuiHint } from '@taiga-ui/core'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  selector: '[appFooter]',
  template: `
    <button
      tuiButton
      type="reset"
      appearance="flat"
      [disabled]="form?.pristine"
    >
      {{ 'Cancel' | i18n }}
    </button>
    <button
      tuiButton
      [disabled]="form?.pristine || blocked()"
      [tuiHint]="blocked()"
    >
      {{ 'Save' | i18n }}
    </button>
    <ng-content />
    @if (form?.control?.events | async) {}
  `,
  host: { class: 'g-footer' },
  hostDirectives: [TuiAnimated],
  imports: [TuiButton, TuiHint, AsyncPipe, i18nPipe],
})
export class Footer {
  readonly blocked = input<string | null>(null)
  protected readonly form = inject(FormGroupDirective, { optional: true })
}
