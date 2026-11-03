import { Component, computed, inject, signal } from '@angular/core'
import {
  FormsModule,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { TrustContext } from '@start9labs/marketplace'
import { i18nPipe } from '@start9labs/shared'
import { tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  TuiDialogContext,
  TuiError,
  TuiInput,
  TuiLoader,
  TuiNotification,
  TuiTitle,
  tuiValidationErrorsProvider,
} from '@taiga-ui/core'
import { TuiSwitch } from '@taiga-ui/kit'
import { TuiForm, TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { TrustedKeysService } from 'src/app/services/trusted-keys.service'
import { TrustedKeyFacts } from './facts'
import { TRUST_PERMISSIONS } from './permissions'

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
      @for (permission of permissions; track permission.context) {
        <label class="g-toggle">
          <span>{{ permission.label | i18n }}</span>
          @if (changing() === permission.context) {
            <tui-loader size="xs" />
          }
          <input
            tuiSwitch
            type="checkbox"
            size="m"
            [showIcons]="false"
            [disabled]="trust.saving()"
            [ngModel]="key()?.[permission.context] || false"
            [ngModelOptions]="{ standalone: true }"
            (click.prevent)="toggle(permission.context)"
          />
        </label>
      }
    </form>
  `,
  styles: `
    .g-toggle {
      height: auto;
      min-height: var(--tui-height-l);
      padding-block: 0.5rem;
      gap: 0.75rem;
    }

    .g-toggle > span {
      flex: 1;
    }
  `,
  imports: [
    FormsModule,
    ReactiveFormsModule,
    TrustedKeyFacts,
    TuiError,
    TuiForm,
    TuiHeader,
    TuiInput,
    TuiLoader,
    TuiNotification,
    TuiSwitch,
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
  protected readonly permissions = TRUST_PERMISSIONS
  protected readonly changing = signal<TrustContext | null>(null)
  protected readonly key = computed(
    () => this.trust.keys()?.[this.context.data],
  )
  protected readonly form = inject(NonNullableFormBuilder).group({
    name: ['', [Validators.required, Validators.pattern(/\S/)]],
  })

  protected async toggle(context: TrustContext): Promise<void> {
    const key = this.key()

    if (!key) {
      tuiMarkControlAsTouchedAndValidate(this.form)
      if (!this.form.valid) return
    }

    this.changing.set(context)
    try {
      if (key) {
        await this.trust.trust(this.context.data, key, context, !key[context])
      } else {
        await this.trust.add(
          this.context.data,
          this.form.getRawValue().name,
          context,
        )
      }
    } finally {
      this.changing.set(null)
    }
  }
}

export const TRUSTED_KEY_DETAILS = new PolymorpheusComponent(TrustedKeyDetails)
