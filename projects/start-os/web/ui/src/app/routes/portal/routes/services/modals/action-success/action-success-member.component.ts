import { Component, inject, Input, TemplateRef } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { CopyService, DialogService, i18nPipe } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiButton, TuiInput, TuiTitle } from '@taiga-ui/core'
import { QRComponent } from 'src/app/components/qr.component'

@Component({
  selector: 'app-action-success-member',
  template: `
    <tui-textfield>
      <label tuiLabel>{{ member.name }}</label>
      <input
        tuiInput
        [readOnly]="true"
        [ngModel]="member.value"
        [type]="member.masked && masked ? 'password' : 'text'"
      />
      @if (member.masked) {
        <button
          tuiIconButton
          appearance="icon"
          size="s"
          type="button"
          tabindex="-1"
          [iconStart]="masked ? '@tui.eye' : '@tui.eye-off'"
          [style.pointer-events]="'auto'"
          (click)="masked = !masked"
        >
          {{ 'Reveal/Hide' | i18n }}
        </button>
      }
      @if (member.copyable) {
        <button
          tuiIconButton
          appearance="icon"
          size="s"
          type="button"
          tabindex="-1"
          iconStart="@tui.copy"
          [style.pointer-events]="'auto'"
          (click)="copy.copy(member.value)"
        >
          {{ 'Copy' | i18n }}
        </button>
      }
      @if (member.qr) {
        <button
          tuiIconButton
          appearance="icon"
          size="s"
          type="button"
          tabindex="-1"
          iconStart="@tui.qr-code"
          [style.pointer-events]="'auto'"
          (click)="show(qr)"
        >
          {{ 'Show QR' | i18n }}
        </button>
      }
    </tui-textfield>
    @if (member.description) {
      <label [style.padding-top.rem]="0.25" tuiTitle>
        <span tuiSubtitle [style.opacity]="0.8">{{ member.description }}</span>
      </label>
    }
    <ng-template #qr>
      <app-qr
        [value]="member.value"
        [style.filter]="member.masked && masked ? 'blur(0.5rem)' : null"
      />
      @if (member.masked && masked) {
        <button
          tuiIconButton
          class="reveal"
          iconStart="@tui.eye"
          [style.border-radius.%]="100"
          (click)="masked = false"
        >
          {{ 'Reveal' | i18n }}
        </button>
      }
    </ng-template>
  `,
  styles: `
    @use '@taiga-ui/styles/utils' as taiga;

    .reveal {
      @include taiga.center-all();
    }

    .qr {
      position: relative;
      text-align: center;
    }
  `,
  imports: [FormsModule, TuiInput, TuiButton, QRComponent, TuiTitle, i18nPipe],
})
export class ActionSuccessMemberComponent {
  private readonly dialog = inject(DialogService)
  readonly copy = inject(CopyService)

  @Input()
  member!: T.ActionResultMember & { type: 'single' }

  masked = true

  show(template: TemplateRef<any>) {
    const masked = this.masked

    this.masked = this.member.masked
    this.dialog
      .openComponent(template, { label: 'Scan this QR', size: 's' })
      .subscribe({
        complete: () => (this.masked = masked),
      })
  }
}
