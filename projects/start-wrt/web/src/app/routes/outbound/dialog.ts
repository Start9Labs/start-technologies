import { Component, inject, signal } from '@angular/core'
import { NonNullableFormBuilder, ReactiveFormsModule } from '@angular/forms'
import {
  TuiButton,
  TuiDialogContext,
  TuiError,
  TuiInput,
  TuiTextfield,
  tuiTextfieldOptionsProvider,
} from '@taiga-ui/core'
import { provideTranslatedValidationErrors } from 'src/app/i18n/validation-errors'
import {
  TuiChevron,
  TuiDataListWrapper,
  TuiFile,
  TuiFiles,
  TuiSelect,
} from '@taiga-ui/kit'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { provideHelp } from 'src/app/help/help'
import { ModalHelp } from 'src/app/help/modal-help'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import {
  AddClientDialogData,
  getAddOutboundVpnForm,
  OUTBOUND_VALIDATION_ERRORS,
} from './utils'

@Component({
  template: `
    <form
      tuiForm="m"
      [style.margin-top.rem]="1"
      [formGroup]="form"
      (submit.prevent)="save()"
    >
      <tui-textfield>
        <label tuiLabel>{{ 'Label' | i18n }}</label>
        <input
          tuiInput
          [placeholder]="'e.g. Mullvad Sweden' | i18n"
          formControlName="label"
        />
      </tui-textfield>
      <tui-error formControlName="label" />
      @if (!form.value.config || !form.controls.config.valid) {
        <label tuiInputFiles class="g-action" [style.min-block-size.rem]="6">
          <input tuiInputFiles accept=".conf" formControlName="config" />
          <ng-template>
            <div>
              {{ 'Drop WireGuard' | i18n }}
              <b>.conf</b>
              {{ 'file here' | i18n }}
              <div>{{ 'or click to browse' | i18n }}</div>
            </div>
          </ng-template>
        </label>
      } @else {
        <tui-file
          [file]="form.value.config"
          (remove)="form.controls.config.reset()"
        />
      }
      <tui-error formControlName="config" />
      <tui-textfield tuiChevron [stringify]="stringifyTarget">
        <label tuiLabel>{{ 'Target' | i18n }}</label>
        <input tuiSelect formControlName="target" />
        <tui-data-list-wrapper *tuiDropdown [items]="context.data.targets" />
      </tui-textfield>
      <footer>
        <button
          tuiButton
          type="button"
          appearance="flat"
          (click)="context.$implicit.complete()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button tuiButton>{{ 'Add VPN' | i18n }}</button>
      </footer>
    </form>
  `,
  hostDirectives: [ModalHelp],
  providers: [
    provideHelp('/outbound/dialog'),
    tuiTextfieldOptionsProvider({ cleaner: signal(false) }),
    provideTranslatedValidationErrors(OUTBOUND_VALIDATION_ERRORS),
  ],
  imports: [
    TuiForm,
    TuiTextfield,
    TuiError,
    ReactiveFormsModule,
    TuiFiles,
    TuiFile,
    TuiSelect,
    TuiDataListWrapper,
    TuiChevron,
    TuiButton,
    TuiInput,
    i18nPipe,
  ],
})
class AddClient {
  private readonly i18n = inject(i18nPipe)
  protected readonly context =
    injectContext<TuiDialogContext<any, AddClientDialogData>>()
  protected readonly form = getAddOutboundVpnForm(
    inject(NonNullableFormBuilder),
    this.context.data.existingLabels,
  )

  // Translates the 'Internet' option; user VPN labels pass through unchanged.
  protected readonly stringifyTarget = (v: string): string =>
    this.i18n.transform(v)

  constructor() {
    // The config control uses an async validator (WireGuard content check), so
    // its status settles to INVALID *after* the value changes — listen on
    // statusChanges, not valueChanges, or the error never gets marked touched
    // and stays hidden. Treating PENDING as "not valid yet" in the template (so
    // the file input isn't unmounted mid-validation) is what prevents the
    // mount/remount loop that made the dialog flicker.
    this.form.controls.config.statusChanges.subscribe(status => {
      if (status === 'INVALID') {
        this.form.controls.config.markAsTouched()
      }
    })
  }

  protected save(): void {
    // Once a valid .conf is dropped the async WireGuard validator settles to
    // VALID, so the form is already valid here — submit directly. Do NOT call
    // tuiMarkControlAsTouchedAndValidate in that case: it runs
    // updateValueAndValidity, which re-triggers the async validator, and that
    // re-run gets stuck PENDING and never re-settles (the in-flight validation is
    // cancelled when the file input remounts during the PENDING phase), silently
    // blocking submit forever.
    if (this.form.valid) {
      this.context.completeWith(this.form.value)
      return
    }

    // Otherwise the form is incomplete/invalid — surface the errors. markAllAsTouched
    // only flips touched state; unlike tuiMarkControlAsTouchedAndValidate it does
    // not re-run validators, so it can't get stuck on the async one. Validators have
    // already run on input, so the errors are present and just need to be shown.
    this.form.markAllAsTouched()
  }
}

export const ADD_CLIENT = new PolymorpheusComponent(AddClient)
