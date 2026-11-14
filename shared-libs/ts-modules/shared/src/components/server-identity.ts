import { Component, inject } from '@angular/core'
import {
  TuiButton,
  TuiDialogContext,
  TuiNotificationOptions,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'

import { i18nPipe } from '../i18n/i18n.pipe'
import { CopyService } from '../services/copy.service'
import { DialogService } from '../services/dialog.service'

interface ServerIdentityChange {
  readonly target: string
  readonly previous: string
  readonly current: string
}

@Component({
  template: `
    <strong>{{ 'Server identity changed' | i18n }}</strong>
    <p>{{ context.data.target }}</p>
    <p>
      {{
        'This can happen after replacing or restoring a server, but could also indicate interception. Verify unexpected changes through a trusted channel.'
          | i18n
      }}
    </p>
    <p>
      {{
        'The connection continues. The new identity is now remembered.' | i18n
      }}
    </p>
    <button
      tuiButton
      type="button"
      appearance="flat"
      size="s"
      (click)="details()"
    >
      {{ 'Details' | i18n }}
    </button>
  `,
  imports: [TuiButton, i18nPipe],
})
class ServerIdentityWarning {
  private readonly dialogs = inject(DialogService)
  protected readonly context =
    injectContext<TuiNotificationOptions<ServerIdentityChange>>()

  protected details() {
    this.dialogs
      .openComponent(SERVER_IDENTITY_DETAILS, { data: this.context.data })
      .subscribe()
  }
}

@Component({
  template: `
    <header tuiHeader>
      <h2 tuiTitle [id]="context.id">{{ 'Server identity changed' | i18n }}</h2>
    </header>
    <p>
      {{
        'This server is using a different identity key than the one this browser remembers.'
          | i18n
      }}
    </p>
    <p>
      {{
        'This can happen after replacing or restoring a server, but could also indicate interception. Verify unexpected changes through a trusted channel.'
          | i18n
      }}
    </p>
    <p>
      <b>{{ 'Address' | i18n }}</b>
      <br />
      <code>{{ context.data.target }}</code>
    </p>
    @for (entry of fingerprints; track entry.label) {
      <p>
        <b>{{ entry.label | i18n }}</b>
        <br />
        <code>{{ entry.value }}</code>
        <button
          tuiButton
          type="button"
          appearance="flat-grayscale"
          size="s"
          (click)="copy.copy(entry.value)"
        >
          {{ 'Copy fingerprint' | i18n }}
        </button>
      </p>
    }
    <p>
      {{
        'The connection continues. The new identity is now remembered.' | i18n
      }}
    </p>
    <footer class="g-buttons">
      <button tuiButton type="button" (click)="context.completeWith()">
        {{ 'Close' | i18n }}
      </button>
    </footer>
  `,
  styles: `
    code {
      overflow-wrap: anywhere;
    }
  `,
  imports: [TuiButton, TuiHeader, TuiTitle, i18nPipe],
})
class ServerIdentityDetails {
  protected readonly context =
    injectContext<TuiDialogContext<void, ServerIdentityChange>>()
  protected readonly copy = inject(CopyService)
  protected readonly fingerprints = [
    { label: 'Previous fingerprint', value: this.context.data.previous },
    { label: 'Current fingerprint', value: this.context.data.current },
  ] as const
}

const SERVER_IDENTITY_DETAILS = new PolymorpheusComponent(ServerIdentityDetails)
export const SERVER_IDENTITY_WARNING = new PolymorpheusComponent(
  ServerIdentityWarning,
)
