import {
  Component,
  computed,
  inject,
  linkedSignal,
  TemplateRef,
} from '@angular/core'
import { DialogService, i18nPipe } from '@start9labs/shared'
import { TuiResponsiveDialog } from '@taiga-ui/addon-mobile'
import { TuiButton } from '@taiga-ui/core'
import { TuiButtonLoading } from '@taiga-ui/kit'
import { defer, of } from 'rxjs'

import { UiUpdateService } from 'src/app/services/ui-update.service'

@Component({
  selector: 'refresh-alert',
  template: `
    <ng-template
      [tuiResponsiveDialog]="show()"
      [tuiResponsiveDialogOptions]="{
        label: i18n.transform('Refresh Needed'),
        size: 's',
        closable: canClose$,
        dismissible: canClose$,
      }"
      (tuiResponsiveDialogChange)="dismissed.set(!$event)"
    >
      @let state = ui.state();
      @if (!ui.refreshing() && ui.outdated()) {
        <p>
          {{
            'StartOS has been updated, but this page is still running the previous interface. Refresh the page to get the latest version.'
              | i18n
          }}
        </p>
      }
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
              'The interface update failed. Refresh the page, retry, or reset the UI cache.'
                | i18n
            }}
          }
          @case ('resetting') {
            {{ 'Resetting UI cache...' | i18n }}
          }
          @case ('reloading') {
            {{ 'Loading' | i18n }}
          }
          @default {
            {{ 'Checking for updates' | i18n }}
          }
        }
      </p>
      <button
        tuiButton
        size="s"
        [loading]="ui.refreshing()"
        [disabled]="ui.refreshing()"
        (click)="ui.refresh()"
      >
        {{ 'Refresh' | i18n }}
      </button>
      @if (
        state.status !== 'ready' &&
        state.status !== 'resetting' &&
        state.status !== 'reloading'
      ) {
        @if (state.status === 'error' && !ui.refreshing()) {
          <button tuiButton size="s" appearance="flat" (click)="ui.check()">
            {{ 'Retry' | i18n }}
          </button>
        }
        <button tuiButton size="s" appearance="flat" (click)="resetCache()">
          {{ 'Reset UI cache' | i18n }}
        </button>
        @if (state.status === 'error' && state.error && !ui.refreshing()) {
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
  imports: [TuiResponsiveDialog, TuiButton, TuiButtonLoading, i18nPipe],
})
export class RefreshAlertComponent {
  private readonly dialogs = inject(DialogService)

  protected readonly ui = inject(UiUpdateService)
  protected readonly i18n = inject(i18nPipe)
  protected readonly canClose$ = defer(() => of(!this.ui.refreshing()))
  protected readonly dismissed = linkedSignal({
    source: () => this.ui.state().status,
    computation: () => false,
  })
  protected readonly show = computed(
    () =>
      this.ui.refreshing() ||
      (!this.dismissed() &&
        (this.ui.outdated() || this.ui.state().status !== 'idle')),
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
          yes: 'Refresh',
          no: 'Cancel',
        },
      })
      .subscribe(confirmed => {
        if (confirmed) void this.ui.resetCache()
      })
  }
}
