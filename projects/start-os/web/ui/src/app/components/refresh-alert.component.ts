import {
  Component,
  computed,
  inject,
  linkedSignal,
  TemplateRef,
} from '@angular/core'
import { DialogService, i18nPipe } from '@start9labs/shared'
import { TuiResponsiveDialog } from '@taiga-ui/addon-mobile'
import { TuiButton, TuiLoader } from '@taiga-ui/core'
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
      @let phase = ui.phase();
      <p role="status" aria-live="polite">
        @switch (phase) {
          @case ('ready') {
            {{ 'User interface synced. Refresh to continue.' | i18n }}
          }
          @case ('error') {
            {{
              'Unable to sync the user interface. Retry or reset the UI cache.'
                | i18n
            }}
          }
          @case ('syncing') {
            {{ 'Syncing the interface with your server...' | i18n }}
          }
        }
      </p>
      @if (phase !== 'error') {
        <button
          tuiButton
          size="s"
          [disabled]="phase !== 'ready' || ui.busy()"
          (click)="ui.refresh()"
        >
          @if (phase === 'syncing' || ui.busy()) {
            <tui-loader size="xs" [inheritColor]="true" />
          }
          {{ (phase === 'syncing' ? 'Syncing' : 'Refresh') | i18n }}
        </button>
      }
      @if (phase !== 'ready') {
        @if (phase === 'error') {
          <button tuiButton size="s" (click)="ui.check()">
            {{ 'Retry' | i18n }}
          </button>
        }
        <button
          tuiButton
          size="s"
          appearance="flat"
          [loading]="ui.busy()"
          [disabled]="ui.busy()"
          (click)="resetCache()"
        >
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
  imports: [
    TuiResponsiveDialog,
    TuiButton,
    TuiLoader,
    TuiButtonLoading,
    i18nPipe,
  ],
})
export class RefreshAlertComponent {
  private readonly dialogs = inject(DialogService)

  protected readonly ui = inject(UiUpdateService)
  protected readonly i18n = inject(i18nPipe)
  protected readonly canClose$ = defer(() => of(!this.ui.busy()))
  protected readonly dismissed = linkedSignal({
    source: this.ui.phase,
    computation: () => false,
  })
  protected readonly show = computed(
    () =>
      this.ui.busy() ||
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
          yes: 'Reset UI cache',
          no: 'Cancel',
        },
      })
      .subscribe(confirmed => {
        if (confirmed) void this.ui.resetCache()
      })
  }
}
