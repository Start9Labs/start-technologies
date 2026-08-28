import { Component, computed, signal } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TuiButton,
  TuiDialogContext,
  TuiInput,
  TuiNotification,
  TuiTitle,
} from '@taiga-ui/core'
import {
  TuiAvatar,
  TuiChevron,
  TuiDataListWrapper,
  TuiSelect,
} from '@taiga-ui/kit'
import { TuiForm } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'

import { StoreIconDirective } from './store-icon.directive'

@Component({
  template: `
    <form tuiForm (submit.prevent)="submit()">
      <div tuiNotification appearance="warning">
        {{
          'Start9 does not operate these registries or support the services they distribute.'
            | i18n
        }}
      </div>

      @if (context.data.length) {
        <fieldset>
          <legend>{{ 'Known Registries' | i18n }}</legend>
          <tui-textfield
            tuiChevron
            [stringify]="stringify"
            [tuiTextfieldCleaner]="false"
          >
            <label tuiLabel>{{ 'Select a registry' | i18n }}</label>
            <input
              tuiSelect
              [(ngModel)]="selected"
              [ngModelOptions]="{ standalone: true }"
              (ngModelChange)="url.set('')"
            />
            <tui-data-list-wrapper
              *tuiDropdown
              [items]="context.data"
              [itemContent]="registryContent"
            />
          </tui-textfield>

          <ng-template #registryContent let-registry>
            <span tuiAvatar><img [storeIcon]="registry.url" /></span>
            <span tuiTitle>
              {{ registry.name }}
              <span tuiSubtitle>{{ registry.url }}</span>
            </span>
          </ng-template>
        </fieldset>
      }

      <fieldset>
        <legend>{{ 'Custom Registry' | i18n }}</legend>
        <tui-textfield>
          <label tuiLabel>{{ 'URL' | i18n }}</label>
          <input
            tuiInput
            autocapitalize="off"
            placeholder="registry.example.com"
            [(ngModel)]="url"
            [ngModelOptions]="{ standalone: true }"
            (ngModelChange)="selected.set(null)"
          />
        </tui-textfield>
      </fieldset>

      <footer>
        <button
          tuiButton
          type="button"
          appearance="secondary"
          (click)="context.$implicit.complete()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button tuiButton type="submit" [disabled]="!value()">
          {{ 'Add' | i18n }}
        </button>
      </footer>
    </form>
  `,
  imports: [
    FormsModule,
    StoreIconDirective,
    TuiAvatar,
    TuiButton,
    TuiChevron,
    TuiDataListWrapper,
    TuiForm,
    TuiInput,
    TuiNotification,
    TuiSelect,
    TuiTitle,
    i18nPipe,
  ],
})
export class MarketplaceAddRegistryComponent {
  protected readonly context =
    injectContext<TuiDialogContext<string, T.KnownRegistry[]>>()

  protected readonly selected = signal<T.KnownRegistry | null>(null)
  protected readonly url = signal('')

  protected readonly value = computed(
    () => this.selected()?.url || this.url().trim(),
  )

  protected readonly stringify = (registry: T.KnownRegistry | null) =>
    registry?.name || ''

  protected submit(): void {
    if (this.value()) this.context.completeWith(this.value())
  }
}

export const ADD_REGISTRY = new PolymorpheusComponent(
  MarketplaceAddRegistryComponent,
)
