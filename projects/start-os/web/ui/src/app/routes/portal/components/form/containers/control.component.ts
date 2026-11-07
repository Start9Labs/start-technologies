import { Component, inject, Input } from '@angular/core'
import { ReactiveFormsModule } from '@angular/forms'
import { convertBytes, DialogService, i18nPipe } from '@start9labs/shared'
import { IST } from '@start9labs/start-core'
import { tuiAsControl, TuiControl } from '@taiga-ui/cdk'
import { TUI_VALIDATION_ERRORS, TuiError } from '@taiga-ui/core'
import { TUI_FORMAT_ERROR, TUI_SIZE_ERROR } from '@taiga-ui/kit'
import { PolymorpheusOutlet } from '@taiga-ui/polymorpheus'

import { ControlSpec } from '../controls/control'
import { CONTROLS } from '../controls/controls'
import { FootnotePipe } from '../pipes/footnote.pipe'
import { ControlDirective } from './control.directive'

export const ERRORS = [
  TUI_FORMAT_ERROR,
  TUI_SIZE_ERROR,
  'required',
  'pattern',
  'notNumber',
  'numberNotInteger',
  'numberNotInRange',
  'listNotUnique',
  'listNotInRange',
  'listItemIssue',
]

@Component({
  selector: 'form-control',
  template: `
    <ng-container *polymorpheusOutlet="controls[spec.type]" />
    <tui-error [formControl]="$any(control.control)" [order]="order" />
    @if (spec | footnote; as footnote) {
      <tui-error class="g-secondary" [error]="footnote" />
    }
  `,
  providers: [
    tuiAsControl(FormControlComponent),
    {
      provide: TUI_VALIDATION_ERRORS,
      deps: [FormControlComponent],
      useFactory: (control: FormControlComponent<ControlSpec, string>) => {
        const i18n = inject(i18nPipe)
        return {
          [TUI_FORMAT_ERROR]: i18n.transform('Invalid file format'),
          [TUI_SIZE_ERROR]: ({ size }: { size: number }) =>
            `${i18n.transform('Maximum file size')}: ${convertBytes(size)}`,
          required: i18n.transform('Required'),
          pattern: (context: any) =>
            'patterns' in control.spec &&
            getText(control.spec, String(context.requiredPattern)),
        }
      },
    },
  ],
  hostDirectives: [ControlDirective],
  imports: [PolymorpheusOutlet, TuiError, ReactiveFormsModule, FootnotePipe],
})
export class FormControlComponent<
  T extends ControlSpec,
  V,
> extends TuiControl<V | null> {
  private readonly dialogs = inject(DialogService)
  private readonly i18n = inject(i18nPipe)

  protected readonly controls = CONTROLS

  @Input({ required: true })
  spec!: T

  warned = false
  readonly order = ERRORS

  onInput(value: V | null) {
    const previous = this.value()

    let warning = this.spec.warning

    const immutable =
      'immutable' in this.spec &&
      this.spec.immutable &&
      `${this.i18n.transform('This value cannot be changed once set')}.`

    if (immutable) {
      warning = warning
        ? `<ul><li>${warning}</li><li>${immutable}</li></ul>`
        : immutable
    }

    if (!this.warned && warning) {
      this.dialogs
        .openConfirm({
          label: 'Warning',
          data: { content: warning as any, yes: 'Confirm', no: 'Cancel' },
          closable: false,
          dismissible: false,
        })
        .subscribe(confirm => {
          if (!confirm) {
            this.onChange(previous)
          }
        })
    }

    this.warned = true
    this.onChange(value === '' ? null : value)
  }
}

function getText(
  { patterns }: IST.ValueSpecText | IST.ValueSpecTextarea,
  pattern: unknown,
): string {
  return (
    patterns?.find(({ regex }) => String(regex) === pattern)?.description ||
    'Invalid format'
  )
}
