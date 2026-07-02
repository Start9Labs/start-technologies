import { Component, input } from '@angular/core'
import { ReactiveFormsModule } from '@angular/forms'
import { TuiError, TuiInput, TuiTextfield } from '@taiga-ui/core'
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'
import { TuiCardLarge, TuiForm } from '@taiga-ui/layout'
import { FORM } from 'src/app/components/form'
import { DDNS_FIELD_LABELS, DDNS_VALIDATION_ERRORS } from '../utils'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  selector: 'ddns-fields',
  template: `
    <fieldset>
      <legend>{{ 'Credentials' | i18n }}</legend>
      <section>
        @for (field of fields(); track field) {
          <div>
            <tui-textfield>
              <label tuiLabel>{{ fieldLabels[field] | i18n }}</label>
              <input
                tuiInput
                [formControlName]="field"
                [type]="
                  ['password', 'token'].includes(field) ? 'password' : 'text'
                "
              />
            </tui-textfield>
            <tui-error [formControlName]="field" />
          </div>
        }
      </section>
    </fieldset>
  `,
  viewProviders: [FORM],
  hostDirectives: [TuiForm, TuiCardLarge],
  providers: [provideTranslatedValidationErrors(DDNS_VALIDATION_ERRORS)],
  imports: [ReactiveFormsModule, TuiTextfield, TuiError, TuiInput, i18nPipe],
})
export class DdnsFields {
  protected readonly fieldLabels = DDNS_FIELD_LABELS

  readonly fields = input.required<string[]>()
}
