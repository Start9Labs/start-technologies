import { Component, computed, inject } from '@angular/core'
import {
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { TrustContext } from '@start9labs/marketplace'
import { i18nPipe } from '@start9labs/shared'
import { tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TuiButton,
  TuiDialogContext,
  TuiError,
  TuiInput,
  TuiNotification,
  TuiTitle,
  tuiValidationErrorsProvider,
} from '@taiga-ui/core'
import { TuiForm, TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { TrustedKeysService } from 'src/app/services/trusted-keys.service'
import { TrustedKeyFacts } from './facts'

@Component({
  template: `
    <header tuiHeader>
      <h2 tuiTitle [id]="context.id">
        {{ key()?.name || ('Unknown key' | i18n) }}
      </h2>
    </header>
    <form
      tuiForm="m"
      [formGroup]="form"
      (submit.prevent)="toggle('marketplace')"
    >
      <trusted-key-facts [publicKey]="context.data" [key]="key()" />
      @if (!key()?.marketplace || !key()?.os) {
        <p tuiNotification appearance="warning">
          {{
            'Anyone can sign a package. Only trust this key after confirming its fingerprint through a source you already trust, such as the developer’s website, a signed release announcement, or the developer in person. A registry or the package itself cannot vouch for a key.'
              | i18n
          }}
        </p>
      }
      @if (!key()) {
        <tui-textfield>
          <label tuiLabel>{{ 'Label' | i18n }}</label>
          <input tuiInput formControlName="name" />
        </tui-textfield>
        <tui-error formControlName="name" />
      }
      <footer>
        <button
          tuiButton
          type="button"
          [appearance]="key()?.marketplace ? 'secondary' : 'primary'"
          [disabled]="trust.saving()"
          (click)="toggle('marketplace')"
        >
          {{
            (key()?.marketplace
              ? 'Distrust for packages'
              : 'Trust for packages'
            ) | i18n
          }}
        </button>
        <button
          tuiButton
          type="button"
          [appearance]="key()?.os ? 'secondary' : 'primary'"
          [disabled]="trust.saving()"
          (click)="toggle('os')"
        >
          {{
            (key()?.os
              ? 'Distrust for StartOS updates'
              : 'Trust for StartOS updates'
            ) | i18n
          }}
        </button>
      </footer>
    </form>
  `,
  styles: `
    :host-context(tui-root._mobile) footer {
      flex-direction: column;
    }
  `,
  imports: [
    ReactiveFormsModule,
    TrustedKeyFacts,
    TuiButton,
    TuiError,
    TuiForm,
    TuiHeader,
    TuiInput,
    TuiNotification,
    TuiTitle,
    i18nPipe,
  ],
  providers: [
    tuiValidationErrorsProvider(() => {
      const i18n = inject(i18nPipe)
      return {
        required: i18n.transform('Required'),
        pattern: i18n.transform('Required'),
      }
    }),
  ],
})
class TrustedKeyDetails {
  protected readonly context = injectContext<TuiDialogContext<void, string>>()
  protected readonly trust = inject(TrustedKeysService)
  protected readonly key = computed(
    () => this.trust.keys()?.[this.context.data],
  )
  protected readonly form = inject(NonNullableFormBuilder).group({
    name: ['', [Validators.required, Validators.pattern(/\S/)]],
  })

  protected async toggle(context: TrustContext): Promise<void> {
    const key = this.key()

    if (key) {
      await this.trust.trust(this.context.data, key, context, !key[context])
    } else {
      tuiMarkControlAsTouchedAndValidate(this.form)
      if (this.form.valid) {
        await this.trust.add(
          this.context.data,
          this.form.getRawValue().name,
          context,
        )
      }
    }
  }
}

export const TRUSTED_KEY_DETAILS = new PolymorpheusComponent(TrustedKeyDetails)
