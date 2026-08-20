import { Component, inject, Input, OnInit } from '@angular/core'
import { FormGroup, ReactiveFormsModule } from '@angular/forms'
import { RouterModule } from '@angular/router'
import { IST } from '@start9labs/start-core'
import {
  tuiMarkControlAsTouchedAndValidate,
  TuiValueChanges,
} from '@taiga-ui/cdk'
import { TuiButton, TuiDialogContext, TuiNotification } from '@taiga-ui/core'
import { TuiConfirmService } from '@taiga-ui/kit'
import { injectContext } from '@taiga-ui/polymorpheus'
import { Operation } from 'fast-json-patch'
import { FormGroupComponent } from 'src/app/routes/portal/components/form/containers/group.component'
import { InvalidService } from 'src/app/routes/portal/components/form/containers/control.directive'
import { FormService, trim } from 'src/app/services/form.service'

export interface ActionButton<T> {
  text: string
  handler?: (value: T) => Promise<boolean | void> | void
  link?: string
}

export interface FormNote {
  text: string
  items?: string[]
}

export interface FormContext<T> {
  spec: IST.InputSpec
  buttons: ActionButton<T>[]
  value?: T
  operations?: Operation[]
  note?: FormNote
}

@Component({
  selector: 'app-form',
  template: `
    <form
      [formGroup]="form"
      (submit.capture.prevent)="(0)"
      (reset.capture.prevent.stop)="onReset()"
      (tuiValueChanges)="markAsDirty()"
    >
      <form-group [spec]="spec" />
      @if (note) {
        <div tuiNotification appearance="warning" class="note">
          {{ note.text }}
          @if (note.items?.length) {
            <ul>
              @for (item of note.items; track $index) {
                <li>{{ item }}</li>
              }
            </ul>
          }
        </div>
      }
      <footer>
        <ng-content />
        @for (button of buttons; track $index) {
          @if (button.handler) {
            <button
              tuiButton
              [appearance]="$last ? 'primary' : 'flat-grayscale'"
              [type]="$last ? 'submit' : 'button'"
              (click)="onClick(button.handler)"
            >
              {{ button.text }}
            </button>
          } @else {
            <a
              tuiButton
              appearance="flat-grayscale"
              [routerLink]="button.link"
              (click)="close()"
            >
              {{ button.text }}
            </a>
          }
        }
      </footer>
    </form>
  `,
  styles: `
    .note {
      margin-top: 1rem;
    }

    .note ul {
      margin: 0.25rem 0 0;
      padding-inline-start: 1.5rem;
      list-style: disc;
    }

    footer {
      position: sticky;
      bottom: 0;
      z-index: 10;
      display: flex;
      justify-content: flex-end;
      padding: 1rem 0;
      margin: 1rem -1px -1rem;
      gap: 1rem;
      background: var(--tui-background-elevation-1);
      box-shadow: inset 0 1px var(--tui-background-neutral-1);
    }
  `,
  imports: [
    ReactiveFormsModule,
    RouterModule,
    TuiValueChanges,
    TuiButton,
    TuiNotification,
    FormGroupComponent,
  ],
  providers: [InvalidService],
})
export class FormComponent<T extends Record<string, any>> implements OnInit {
  private readonly confirm = inject(TuiConfirmService, { optional: true })
  private readonly formService = inject(FormService)
  private readonly invalidService = inject(InvalidService)
  private readonly context = injectContext<
    TuiDialogContext<void, FormContext<T>>
  >({ optional: true })

  @Input() spec = this.context?.data.spec || {}
  @Input() buttons = this.context?.data.buttons || []
  @Input() operations = this.context?.data.operations || []
  @Input() value?: T = this.context?.data.value
  @Input() note = this.context?.data.note

  form = new FormGroup({})

  ngOnInit() {
    this.confirm?.markAsPristine()
    this.form = this.formService.createForm(this.spec, this.value)
    this.process(this.operations)
  }

  onReset() {
    this.form = this.formService.createForm(this.spec)
    tuiMarkControlAsTouchedAndValidate(this.form)
    this.markAsDirty()
  }

  async onClick(handler: Required<ActionButton<T>>['handler']) {
    trim(this.form)
    tuiMarkControlAsTouchedAndValidate(this.form)
    this.invalidService.scrollIntoView()

    if (this.form.valid && (await handler(this.form.value as T))) {
      this.close()
    }
  }

  markAsDirty() {
    this.confirm?.markAsDirty()
  }

  close() {
    this.context?.$implicit.complete()
  }

  private process(operations: Operation[]) {
    operations.forEach(operation => {
      const control = this.form.get(operation.path.substring(1).split('/'))

      if (!control || !control.parent) return

      if (operation.op === 'add' || operation.op === 'replace') {
        control.markAsDirty()
        control.markAsTouched()
        control.setValue(operation.value)
      }

      control.parent.markAsDirty()
      control.parent.markAsTouched()
    })
  }
}
