import { Component, computed, inject, signal } from '@angular/core'
import { TuiResponsiveDialog } from '@taiga-ui/addon-mobile'
import { TuiAutoFocus } from '@taiga-ui/cdk'
import { TuiButton } from '@taiga-ui/core'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'
import { StaleUiService } from 'src/app/services/stale-ui.service'

/**
 * Passive stale-UI prompt (see StaleUiService): shown when the firmware was
 * updated outside this tab (CLI deploy, another device). index.html
 * revalidates its ETag on every load, so a plain reload is guaranteed to
 * fetch the matching bundle — no hard refresh needed.
 */
@Component({
  selector: 'stale-ui-alert',
  template: `
    <ng-template
      [tuiResponsiveDialog]="show()"
      [tuiResponsiveDialogOptions]="{
        label: i18n.transform('Refresh Needed'),
        size: 's',
      }"
      (tuiResponsiveDialogChange)="dismissedFor.set(staleHash())"
    >
      <p>
        {{
          'The router firmware has been updated, but this page is still running the previous interface. Reload the page to get the latest version.'
            | i18n
        }}
      </p>
      <button
        tuiButton
        tuiAutoFocus
        appearance="secondary"
        style="float: right"
        [tuiAppearanceFocus]="false"
        (click)="reload()"
      >
        {{ 'Reload' | i18n }}
      </button>
    </ng-template>
  `,
  imports: [TuiResponsiveDialog, TuiButton, TuiAutoFocus, i18nPipe],
})
export class StaleUiAlert {
  protected readonly i18n = inject(i18nPipe)
  protected readonly staleHash = inject(StaleUiService).staleHash
  /**
   * The build the user dismissed the prompt for — dismissal silences that
   * build only, so yet another firmware update re-prompts.
   */
  protected readonly dismissedFor = signal<string | null>(null)
  protected readonly show = computed(
    () => this.staleHash() !== null && this.staleHash() !== this.dismissedFor(),
  )

  protected reload(): void {
    window.location.reload()
  }
}
