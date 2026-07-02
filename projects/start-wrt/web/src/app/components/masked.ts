import { CdkCopyToClipboard } from '@angular/cdk/clipboard'
import { Component, inject, input, linkedSignal } from '@angular/core'
import { TuiButton, TuiNotificationService } from '@taiga-ui/core'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  selector: '[appMasked]',
  template: `
    {{ masked() ? '••••••••••••••••' : appMasked() }}
    <button
      tuiIconButton
      size="xs"
      appearance="icon"
      [iconStart]="masked() ? '@tui.eye' : '@tui.eye-off'"
      (click)="masked.set(!masked())"
    >
      {{ 'Toggle visibility' | i18n }}
    </button>
    <button
      tuiIconButton
      size="xs"
      appearance="icon"
      iconStart="@tui.copy"
      [cdkCopyToClipboard]="appMasked()"
      (cdkCopyToClipboardCopied)="
        alerts
          .open(i18n.transform('Copied!'), { appearance: 'positive' })
          .subscribe()
      "
    >
      {{ 'Copy' | i18n }}
    </button>
  `,
  imports: [TuiButton, CdkCopyToClipboard, i18nPipe],
})
export class Masked {
  public readonly appMasked = input('')

  protected readonly alerts = inject(TuiNotificationService)
  protected readonly i18n = inject(i18nPipe)
  protected readonly masked = linkedSignal({
    source: this.appMasked,
    computation: () => true,
  })
}
