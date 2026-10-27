import {
  Component,
  computed,
  inject,
  linkedSignal,
  TemplateRef,
} from '@angular/core'
import { DialogService, i18nPipe } from '@start9labs/shared'
import { TuiButton, TuiNotification } from '@taiga-ui/core'

import { UiUpdateService } from 'src/app/services/ui-update.service'

@Component({
  selector: 'refresh-alert',
  template: `
    <ng-template
      [tuiNotification]="show()"
      [tuiNotificationOptions]="{
        label: i18n.transform('Interface update'),
        autoClose: 0,
      }"
      (tuiNotificationChange)="dismissed.set(!$event)"
    >
      @let state = ui.state();
      <p role="status" aria-live="polite">
        @switch (state.status) {
          @case ('ready') {
            {{ 'The updated interface is ready. Refresh to use it.' | i18n }}
          }
          @case ('updating') {
            {{ 'Updating interface...' | i18n }}
          }
          @case ('error') {
            {{
              'The interface update failed. Retry or reset the UI cache.' | i18n
            }}
          }
          @case ('resetting') {
            {{ 'Resetting UI cache...' | i18n }}
          }
          @default {
            {{ 'Checking for updates' | i18n }}
          }
        }
      </p>
      @if (state.status === 'ready') {
        <button tuiButton size="s" (click)="ui.reload()">
          {{ 'Refresh' | i18n }}
        </button>
      } @else if (state.status !== 'resetting') {
        @if (state.status === 'error') {
          <button tuiButton size="s" (click)="ui.check()">
            {{ 'Retry' | i18n }}
          </button>
        }
        <button tuiButton size="s" appearance="flat" (click)="resetCache()">
          {{ 'Reset UI cache' | i18n }}
        </button>
        @if (state.status === 'error' && state.error) {
          <ng-template #details>
            <p>{{ state.error }}</p>
          </ng-template>
          <button
            tuiButton
            size="s"
            appearance="flat"
            (click)="openDetails(details)"
          >
            {{ 'Details' | i18n }}
          </button>
        }
      }
    </ng-template>
  `,
  imports: [TuiNotification, TuiButton, i18nPipe],
})
export class RefreshAlertComponent {
  private readonly dialogs = inject(DialogService)

  protected readonly ui = inject(UiUpdateService)
  protected readonly i18n = inject(i18nPipe)
  protected readonly dismissed = linkedSignal({
    source: () => this.ui.state().status,
    computation: () => false,
  })
  protected readonly show = computed(
    () =>
      !this.dismissed() &&
      (this.ui.outdated() || this.ui.state().status !== 'idle'),
  )

  protected openDetails(content: TemplateRef<unknown>): void {
    this.dialogs.openComponent(content, { label: 'Error' }).subscribe()
  }

  protected resetCache(): void {
    this.dialogs
      .openConfirm({
        label: 'Reset UI cache',
        data: {
          content:
            'Reset the cached interface and reload? Your login and settings will be kept.',
          yes: 'Reset UI cache',
          no: 'Cancel',
        },
      })
      .subscribe(confirmed => {
        if (confirmed) void this.ui.resetCache()
      })
  }
}
