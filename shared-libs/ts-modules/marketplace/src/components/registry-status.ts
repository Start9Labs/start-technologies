import { Component, computed, inject, input, Pipe } from '@angular/core'
import { DialogService, i18nKey, i18nPipe } from '@start9labs/shared'
import type { T } from '@start9labs/start-core'
import {
  TuiButton,
  TuiDialogContext,
  TuiTitle,
  tuiAppearance,
} from '@taiga-ui/core'
import { TuiBadge } from '@taiga-ui/kit'
import { TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'

const registryStates = {
  connecting: {
    label: 'Connecting',
    appearance: 'info',
    color: 'var(--tui-status-info)',
  },
  connected: {
    label: 'Connected',
    appearance: 'positive',
    color: 'var(--tui-status-positive)',
  },
  error: {
    label: 'Unreachable',
    appearance: 'negative',
    color: 'var(--tui-status-negative)',
  },
} as const satisfies Record<
  T.RegistryStatus,
  { label: i18nKey; appearance: string; color: string }
>

@Pipe({ name: 'registryState' })
export class RegistryStatePipe {
  transform(status: T.RegistryStatus) {
    return registryStates[status]
  }
}

@Component({
  selector: 'span[marketplaceRegistryStatus]',
  host: { tuiBadge: '' },
  hostDirectives: [TuiBadge],
  template: `
    {{ states[status()].label | i18n }}
  `,
  imports: [i18nPipe],
})
export class RegistryStatus {
  private readonly appearance = tuiAppearance(
    computed(() => registryStates[this.status()].appearance),
  )
  protected readonly states = registryStates
  public readonly status = input.required<T.RegistryStatus>({
    alias: 'marketplaceRegistryStatus',
  })
}

@Component({
  selector: 'marketplace-registry-status',
  template: `
    @if (status() === 'error') {
      <button
        tuiBadge
        type="button"
        aria-haspopup="dialog"
        [appearance]="states.error.appearance"
        (click)="showError()"
      >
        {{ states.error.label | i18n }}
      </button>
    } @else {
      <span [marketplaceRegistryStatus]="status()"></span>
    }
  `,
  imports: [RegistryStatus, TuiBadge, i18nPipe],
})
export class RegistryConnectionStatus {
  private readonly dialogs = inject(DialogService)
  protected readonly states = registryStates
  public readonly status = input.required<T.RegistryStatus>()
  public readonly url = input.required<string>()
  public readonly error = input('', {
    transform: (error: string | null | undefined) => error ?? '',
  })

  protected showError() {
    this.dialogs
      .openComponent(REGISTRY_ERROR, {
        data: { url: this.url(), error: this.error() },
      })
      .subscribe()
  }
}

@Component({
  template: `
    <header tuiHeader>
      <h2 tuiTitle [id]="context.id">{{ states.error.label | i18n }}</h2>
    </header>
    <p>{{ context.data.url }}</p>
    @if (context.data.error) {
      <pre>{{ context.data.error }}</pre>
    } @else {
      <p>{{ 'Error details are not available' | i18n }}</p>
    }
    <footer class="g-buttons">
      <button tuiButton type="button" (click)="context.completeWith()">
        {{ 'Close' | i18n }}
      </button>
    </footer>
  `,
  styles: `
    pre {
      white-space: pre-wrap;
      overflow-wrap: anywhere;
    }
  `,
  imports: [TuiHeader, TuiTitle, TuiButton, i18nPipe],
})
class RegistryError {
  protected readonly states = registryStates
  protected readonly context =
    injectContext<TuiDialogContext<void, { url: string; error: string }>>()
}

const REGISTRY_ERROR = new PolymorpheusComponent(RegistryError)
