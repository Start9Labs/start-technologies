import { DatePipe } from '@angular/common'
import { Component, computed, inject, input } from '@angular/core'
import { FormsModule } from '@angular/forms'
import {
  keyIdentities,
  keyType,
  signingKeyIdentity,
} from '@start9labs/marketplace'
import { CopyService, i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiInput } from '@taiga-ui/core'
import { TuiTextarea } from '@taiga-ui/kit'
import { TrustedKeysService } from 'src/app/services/trusted-keys.service'
import { RegistryTitlePipe } from './origin'

/** Everything the server knows about a public key, as read-only fields. */
@Component({
  selector: 'trusted-key-facts',
  template: `
    <tui-textfield>
      <label tuiLabel>{{ 'Type' | i18n }}</label>
      <input tuiInput [readonly]="true" [ngModel]="type()" />
    </tui-textfield>
    @if (key(); as key) {
      @if (identities(); as identities) {
        @if (identities.length) {
          <tui-textfield>
            <label tuiLabel>{{ 'Identities' | i18n }}</label>
            <textarea
              tuiTextarea
              [readonly]="true"
              [min]="identities.length"
              [max]="8"
              [ngModel]="identities.join('\\n')"
            ></textarea>
          </tui-textfield>
        }
      }
      <tui-textfield>
        <label tuiLabel>{{ 'Trusted for' | i18n }}</label>
        <input tuiInput [readonly]="true" [ngModel]="trustedFor()" />
      </tui-textfield>
      <tui-textfield>
        <label tuiLabel>{{ 'Added by' | i18n }}</label>
        <input
          tuiInput
          [readonly]="true"
          [ngModel]="
            key.origin === 'manual'
              ? ('You' | i18n)
              : (key.origin | registryTitle: key.originRegistry)
          "
        />
      </tui-textfield>
      <tui-textfield>
        <label tuiLabel>{{ 'Date added' | i18n }}</label>
        <input
          tuiInput
          [readonly]="true"
          [ngModel]="key.addedAt | date: 'mediumDate'"
        />
      </tui-textfield>
      <tui-textfield>
        <label tuiLabel>{{ 'Linked registries' | i18n }}</label>
        <input tuiInput [readonly]="true" [ngModel]="linkedRegistries()" />
      </tui-textfield>
    }
    <tui-textfield>
      <label tuiLabel>{{ 'Fingerprint' | i18n }}</label>
      <textarea
        tuiTextarea
        class="value"
        [readonly]="true"
        [min]="1"
        [max]="4"
        [ngModel]="fingerprints().join('\\n')"
      ></textarea>
      <button
        tuiIconButton
        appearance="icon"
        size="s"
        type="button"
        tabindex="-1"
        iconStart="@tui.copy"
        (pointerdown.stop)="(0)"
        (click)="copy.copy(fingerprints().join('\\n'))"
      >
        {{ 'Copy' | i18n }}
      </button>
    </tui-textfield>
    <tui-textfield>
      <label tuiLabel>{{ 'Public key' | i18n }}</label>
      <textarea
        tuiTextarea
        class="value"
        [readonly]="true"
        [min]="1"
        [max]="4"
        [ngModel]="publicKey()"
      ></textarea>
      <button
        tuiIconButton
        appearance="icon"
        size="s"
        type="button"
        tabindex="-1"
        iconStart="@tui.copy"
        (pointerdown.stop)="(0)"
        (click)="copy.copy(publicKey())"
      >
        {{ 'Copy' | i18n }}
      </button>
    </tui-textfield>
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      gap: 1rem;
    }

    .value {
      word-break: break-all;
    }
  `,
  imports: [
    DatePipe,
    FormsModule,
    RegistryTitlePipe,
    TuiButton,
    TuiInput,
    TuiTextarea,
    i18nPipe,
  ],
})
export class TrustedKeyFacts {
  private readonly i18n = inject(i18nPipe)

  protected readonly trust = inject(TrustedKeysService)
  protected readonly copy = inject(CopyService)
  protected readonly type = computed(() =>
    keyType(this.key() ?? { gpgExports: [] }),
  )
  protected readonly identities = computed((key = this.key()) =>
    key ? keyIdentities(key) : [],
  )
  protected readonly fingerprints = computed(
    () => signingKeyIdentity(this.publicKey(), this.key()).fingerprints,
  )
  protected readonly trustedFor = computed((key = this.key()) => {
    const contexts = [
      key?.marketplace && this.i18n.transform('Packages'),
      key?.os && this.i18n.transform('StartOS updates'),
    ].filter(Boolean)

    return contexts.join(', ') || this.i18n.transform('Not trusted')
  })
  protected readonly linkedRegistries = computed(
    () =>
      this.trust
        .offers()
        [this.publicKey()]?.map(offer => offer.name)
        .join(', ') || '—',
  )

  public readonly publicKey = input.required<string>()
  public readonly key = input<T.TrustedKey>()
}
