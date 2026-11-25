import { Component, effect, inject, OnInit } from '@angular/core'
import {
  TuiDialogContext,
  TuiLoader,
  TuiNotificationService,
} from '@taiga-ui/core'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { SystemService } from 'src/app/services/system.service'

/**
 * Blocking progress dialog for a system update. Starts the update on open and
 * closes when it cannot start or once it has finished.
 */
@Component({
  template: `
    <tui-loader size="l" />
    @if (system.rebooting()) {
      {{ 'Device is restarting...' | i18n }}
      <small class="g-secondary">
        {{ 'Your network will be briefly unavailable.' | i18n }}
      </small>
    } @else {
      {{ 'Updating to' | i18n }} v{{ context.data }}...
    }
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 0.5rem;
      padding: 1rem 0;
      text-align: center;
    }
  `,
  imports: [TuiLoader, i18nPipe],
})
export class UpdateProgressDialog implements OnInit {
  protected readonly system = inject(SystemService)
  private readonly alerts = inject(TuiNotificationService)
  private readonly i18n = inject(i18nPipe)
  protected readonly context = injectContext<TuiDialogContext<void, string>>()

  // Set once the update is underway; the idle state before it never closes.
  private active = false

  constructor() {
    effect(() => {
      const busy = this.system.updating() || this.system.rebooting()
      if (busy) {
        this.active = true
      } else if (this.active) {
        this.context.completeWith()
      }
    })
  }

  ngOnInit(): void {
    this.system.startUpdate(this.context.data).catch((e: any) => {
      this.context.completeWith()
      this.alerts
        .open(e?.message || this.i18n.transform('Failed to start update'), {
          appearance: 'negative',
        })
        .subscribe()
    })
  }
}

export const UPDATE_PROGRESS_DIALOG = new PolymorpheusComponent(
  UpdateProgressDialog,
)
