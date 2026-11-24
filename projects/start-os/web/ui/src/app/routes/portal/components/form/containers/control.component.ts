import { Component, inject, Input } from '@angular/core'
import { ReactiveFormsModule } from '@angular/forms'
import {
  convertBytes,
  DialogService,
  escapeHtml,
  i18nPipe,
} from '@start9labs/shared'
import { IST } from '@start9labs/start-core'
import { tuiAsControl, TuiControl } from '@taiga-ui/cdk'
import {
  TUI_VALIDATION_ERRORS,
  TuiDialogContext,
  TuiError,
} from '@taiga-ui/core'
import { TUI_FORMAT_ERROR, TUI_SIZE_ERROR, TuiConfirmData } from '@taiga-ui/kit'
import {
  injectContext,
  PolymorpheusComponent,
  PolymorpheusOutlet,
} from '@taiga-ui/polymorpheus'

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
            escapeHtml(getText(control.spec, String(context.requiredPattern))),
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

  protected readonly controls = CONTROLS

  @Input({ required: true })
  spec!: T

  warned = false
  readonly order = ERRORS

  onInput(value: V | null) {
    const previous = this.value()
    const { warning } = this.spec
    const immutable = 'immutable' in this.spec && this.spec.immutable

    if (!this.warned && (warning || immutable)) {
      const data: WarningData = {
        content: WARNING,
        yes: 'Confirm',
        no: 'Cancel',
        warning,
        immutable,
      }

      this.dialogs
        .openConfirm({
          label: 'Warning',
          data,
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

type WarningData = TuiConfirmData & {
  content: PolymorpheusComponent<ControlWarning>
  warning: string | null
  immutable: boolean | undefined
}

@Component({
  template: `
    @if (context.data.warning; as warning) {
      @if (context.data.immutable) {
        <ul>
          <li>{{ warning }}</li>
          <li>{{ immutable }}</li>
        </ul>
      } @else {
        {{ warning }}
      }
    } @else {
      {{ immutable }}
    }
  `,
  styles: `
    :host {
      display: block;
    }

    :host-context(tui-dialog) {
      margin-block-start: -0.75rem;
    }
  `,
})
class ControlWarning {
  protected readonly context =
    injectContext<TuiDialogContext<boolean, WarningData>>()

  protected readonly immutable = `${inject(i18nPipe).transform('This value cannot be changed once set')}.`
}

const WARNING = new PolymorpheusComponent(ControlWarning)

function getText(
  { patterns }: IST.ValueSpecText | IST.ValueSpecTextarea,
  pattern: unknown,
): string {
  return (
    patterns?.find(({ regex }) => String(regex) === pattern)?.description ||
    'Invalid format'
  )
}
