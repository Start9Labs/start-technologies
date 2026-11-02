import { DatePipe } from '@angular/common'
import { Component, computed, inject } from '@angular/core'
import {
  keyIdentities,
  keyType,
  signingKeyIdentity,
} from '@start9labs/marketplace'
import { CopyService, i18nPipe } from '@start9labs/shared'
import {
  TuiButton,
  TuiCell,
  TuiDialogContext,
  TuiIcon,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { TrustedKeysService } from 'src/app/services/trusted-keys.service'
import { RegistryOrigin } from './origin'

@Component({
  template: `
    @if (key(); as key) {
      <header tuiHeader>
        <hgroup tuiTitle>
          <h2 [id]="context.id">{{ key.name }}</h2>
          <p tuiSubtitle>{{ type() }}</p>
        </hgroup>
      </header>
      <div tuiCell>
        <span tuiTitle>
          <span tuiSubtitle>{{ 'Identities' | i18n }}</span>
          @for (identity of identities(); track identity) {
            <span>{{ identity }}</span>
          } @empty {
            <span>—</span>
          }
        </span>
      </div>
      <div tuiCell>
        <span tuiTitle>
          <span tuiSubtitle>{{ 'Trusted for' | i18n }}</span>
          <span>
            <tui-icon [icon]="key.marketplace ? '@tui.check' : '@tui.x'" />
            {{ 'Packages' | i18n }}
          </span>
          <span>
            <tui-icon [icon]="key.os ? '@tui.check' : '@tui.x'" />
            {{ 'StartOS updates' | i18n }}
          </span>
        </span>
      </div>
      <div tuiCell>
        <span tuiTitle>
          <span tuiSubtitle>{{ 'Added by' | i18n }}</span>
          @if (key.origin === 'manual') {
            <span>{{ 'You' | i18n }}</span>
          } @else {
            <div
              [appRegistryOrigin]="key.origin"
              [identity]="key.originRegistry"
            ></div>
          }
          <span>{{ key.addedAt | date: 'mediumDate' }}</span>
        </span>
      </div>
      <div tuiCell>
        <span tuiTitle>
          <span tuiSubtitle>{{ 'Offered by' | i18n }}</span>
          @for (offer of trust.offers()[context.data]; track offer.url) {
            <span>{{ offer.name }}</span>
          } @empty {
            <span>{{ 'No connected registry' | i18n }}</span>
          }
        </span>
      </div>
      <div tuiCell>
        <span tuiTitle>
          <span tuiSubtitle>{{ 'Fingerprint' | i18n }}</span>
          @for (fingerprint of fingerprints(); track fingerprint) {
            <code>{{ fingerprint }}</code>
          }
        </span>
      </div>
      <button tuiCell type="button" (click)="copy.copy(key.publicKey)">
        <span tuiTitle>
          <span tuiSubtitle>{{ 'Public key' | i18n }}</span>
          <code>{{ key.publicKey }}</code>
        </span>
        <tui-icon icon="@tui.copy" />
      </button>
      <footer class="g-buttons">
        <button
          tuiButton
          size="s"
          appearance="secondary"
          [disabled]="trust.saving()"
          (click)="trust.rename(context.data, key)"
        >
          {{ 'Edit label' | i18n }}
        </button>
        <button
          tuiButton
          size="s"
          appearance="secondary"
          [disabled]="trust.saving()"
          (click)="
            trust.trust(context.data, key, 'marketplace', !key.marketplace)
          "
        >
          {{
            (key.marketplace ? 'Distrust for packages' : 'Trust for packages')
              | i18n
          }}
        </button>
        <button
          tuiButton
          size="s"
          appearance="secondary"
          [disabled]="trust.saving()"
          (click)="trust.trust(context.data, key, 'os', !key.os)"
        >
          {{
            (key.os
              ? 'Distrust for StartOS updates'
              : 'Trust for StartOS updates'
            ) | i18n
          }}
        </button>
        @if (key.removable) {
          <button
            tuiButton
            size="s"
            appearance="secondary-destructive"
            [disabled]="trust.saving()"
            (click)="remove()"
          >
            {{ 'Remove' | i18n }}
          </button>
        }
      </footer>
    }
  `,
  styles: `
    code {
      overflow-wrap: anywhere;
      white-space: normal;
    }

    tui-icon {
      font-size: 1rem;
    }

    footer {
      margin-block-start: 1rem;
    }
  `,
  imports: [
    DatePipe,
    RegistryOrigin,
    TuiButton,
    TuiCell,
    TuiHeader,
    TuiIcon,
    TuiTitle,
    i18nPipe,
  ],
})
class TrustedKeyDetails {
  protected readonly context = injectContext<TuiDialogContext<void, string>>()
  protected readonly trust = inject(TrustedKeysService)
  protected readonly copy = inject(CopyService)
  protected readonly key = computed(
    () => this.trust.keys()?.[this.context.data],
  )
  protected readonly identities = computed((key = this.key()) =>
    key ? keyIdentities(key) : [],
  )
  protected readonly type = computed((key = this.key()) => key && keyType(key))
  protected readonly fingerprints = computed((key = this.key()) =>
    key ? signingKeyIdentity(key.publicKey, key).fingerprints : [],
  )

  protected async remove(): Promise<void> {
    await this.trust.remove(this.context.data)

    if (!this.key()) this.context.$implicit.complete()
  }
}

export const TRUSTED_KEY_DETAILS = new PolymorpheusComponent(TrustedKeyDetails)
