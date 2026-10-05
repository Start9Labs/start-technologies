import {
  Component,
  computed,
  inject,
  input,
  linkedSignal,
  TemplateRef,
} from '@angular/core'
import { CopyService, DialogService, i18nPipe } from '@start9labs/shared'
import { TuiButton, TuiTitle } from '@taiga-ui/core'
import { TuiTextarea } from '@taiga-ui/kit'
import { QRComponent } from 'src/app/routes/portal/components/qr.component'
import { MultilineResult } from './types'

@Component({
  selector: 'app-action-success-multiline',
  template: `
    @if (name()) {
      <span tuiTitle>
        <strong>{{ name() }}</strong>
        @if (description()) {
          <span tuiSubtitle>{{ description() }}</span>
        }
      </span>
    }
    <tui-textfield>
      <textarea
        tuiTextarea
        [max]="16"
        [min]="3"
        [readOnly]="true"
        [style.filter]="masked() ? 'blur(0.5rem)' : null"
        [value]="multiline().value"
      ></textarea>
      @if (multiline().masked) {
        <button
          tuiIconButton
          appearance="icon"
          size="s"
          type="button"
          tabindex="-1"
          [iconStart]="masked() ? '@tui.eye' : '@tui.eye-off'"
          (pointerdown.stop)="(0)"
          (click)="masked.set(!masked())"
        >
          {{ 'Reveal/Hide' | i18n }}
        </button>
      }
      @if (multiline().copyable) {
        <button
          tuiIconButton
          appearance="icon"
          size="s"
          type="button"
          tabindex="-1"
          iconStart="@tui.copy"
          (pointerdown.stop)="(0)"
          (click)="copy.copy(multiline().value)"
        >
          {{ 'Copy' | i18n }}
        </button>
      }
      @if (multiline().qr) {
        <button
          tuiIconButton
          appearance="icon"
          size="s"
          type="button"
          tabindex="-1"
          iconStart="@tui.qr-code"
          (pointerdown.stop)="(0)"
          (click)="show(qr)"
        >
          {{ 'Show QR' | i18n }}
        </button>
      }
      @if (multiline().filename; as filename) {
        <a
          tuiIconButton
          appearance="icon"
          size="s"
          tabindex="-1"
          iconStart="@tui.download"
          [download]="filename"
          [href]="href()"
          (pointerdown.stop)="(0)"
        >
          {{ 'Download' | i18n }}
        </a>
      }
    </tui-textfield>
    <ng-template #qr>
      <app-qr
        [value]="multiline().value"
        [style.filter]="masked() ? 'blur(0.5rem)' : null"
      />
      @if (masked()) {
        <button
          tuiIconButton
          class="reveal"
          iconStart="@tui.eye"
          [style.border-radius.%]="100"
          (click)="masked.set(false)"
        >
          {{ 'Reveal' | i18n }}
        </button>
      }
    </ng-template>
  `,
  styles: `
    @use '@taiga-ui/styles/utils' as taiga;

    :host {
      display: flex;
      flex-direction: column;
      gap: 0.5rem;
    }

    tui-textfield {
      font-family: var(--tui-typography-family-code);
    }

    .reveal {
      @include taiga.center-all();
    }
  `,
  imports: [TuiButton, TuiTextarea, TuiTitle, QRComponent, i18nPipe],
})
export class ActionSuccessMultilineComponent {
  private readonly dialog = inject(DialogService)
  readonly copy = inject(CopyService)

  readonly multiline = input.required<MultilineResult>()
  readonly name = input('')
  readonly description = input('')

  protected readonly masked = linkedSignal(() => this.multiline().masked)
  protected readonly href = computed(() =>
    URL.createObjectURL(
      new Blob([this.multiline().value], { type: 'application/octet-stream' }),
    ),
  )

  protected show(template: TemplateRef<any>) {
    const masked = this.masked()

    this.masked.set(this.multiline().masked)
    this.dialog
      .openComponent(template, { label: 'Scan this QR', size: 's' })
      .subscribe({ complete: () => this.masked.set(masked) })
  }
}
