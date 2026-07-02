import { CdkCopyToClipboard } from '@angular/cdk/clipboard'
import { Component, inject, input, ViewEncapsulation } from '@angular/core'
import {
  TUI_APPEARANCE_OPTIONS,
  TuiAppearance,
  TuiButton,
  tuiButtonOptionsProvider,
  TuiNotificationService,
  TuiTitle,
} from '@taiga-ui/core'
import { tuiBadgeOptionsProvider } from '@taiga-ui/kit'
import { i18nPipe } from 'src/app/i18n/i18n.pipe'

@Component({
  selector: '[appSummary]',
  template: `
    <ng-content />
    @if (appSummary()) {
      <span tuiSubtitle>
        {{ appSummary() }}
        <button
          tuiIconButton
          iconStart="@tui.copy"
          [cdkCopyToClipboard]="appSummary()"
          (cdkCopyToClipboardCopied)="
            alerts
              .open(i18n.transform('Copied'), { appearance: 'positive' })
              .subscribe()
          "
        >
          {{ 'Copy' | i18n }}
        </button>
      </span>
    } @else {
      <ng-content select="[tuiSubtitle]">
        <span tuiSubtitle>-</span>
      </ng-content>
    }
  `,
  styles: `
    [appSummary] {
      padding: 0.5rem 0.75rem;
      border-radius: var(--tui-radius-xs);
      font-weight: bold;

      [tuiIconButton] {
        margin: -0.5rem 0;
      }

      [tuiSubtitle] {
        color: var(--tui-text-secondary);
        display: flex;
        align-items: center;
      }

      > *::first-letter {
        text-transform: capitalize;
      }
    }
  `,
  host: { appSummary: '' },
  hostDirectives: [TuiTitle, TuiAppearance],
  providers: [
    tuiBadgeOptionsProvider({ size: 's' }),
    tuiButtonOptionsProvider({ size: 'xs', appearance: 'icon' }),
    { provide: TUI_APPEARANCE_OPTIONS, useValue: { appearance: 'neutral' } },
  ],
  encapsulation: ViewEncapsulation.None,
  imports: [TuiButton, CdkCopyToClipboard, i18nPipe],
})
export class Summary {
  protected readonly alerts = inject(TuiNotificationService)
  protected readonly i18n = inject(i18nPipe)

  public readonly appSummary = input('', {
    transform: (value?: string) => value || '',
  })
}
