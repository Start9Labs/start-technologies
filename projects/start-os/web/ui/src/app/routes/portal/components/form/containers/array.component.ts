import {
  ChangeDetectorRef,
  Component,
  DestroyRef,
  forwardRef,
  inject,
  Input,
} from '@angular/core'
import { takeUntilDestroyed } from '@angular/core/rxjs-interop'
import {
  AbstractControl,
  FormArrayName,
  FormsModule,
  ReactiveFormsModule,
} from '@angular/forms'
import { DialogService, i18nKey, i18nPipe } from '@start9labs/shared'
import { IST } from '@start9labs/start-core'
import { TuiAnimated } from '@taiga-ui/cdk'
import { TuiButton, TuiError, TuiIcon, TuiInput, TuiLink } from '@taiga-ui/core'
import { TuiTooltip } from '@taiga-ui/kit'
import { filter } from 'rxjs'
import { FormService } from 'src/app/services/form.service'

import { HintPipe } from '../pipes/hint.pipe'
import { MustachePipe } from '../pipes/mustache.pipe'
import { ERRORS, FormControlComponent } from './control.component'
import { ControlDirective } from './control.directive'
import { FormObjectComponent } from './object.component'

@Component({
  selector: 'form-array',
  template: `
    <div class="label">
      {{ spec.name }}
      @if (spec.description || spec.disabled) {
        <tui-icon [tuiTooltip]="spec | hint" />
      }
      <button
        tuiLink
        type="button"
        class="add"
        [disabled]="!canAdd"
        (click)="add()"
      >
        + {{ 'Add' | i18n }}
      </button>
    </div>
    <tui-error
      [ngModel]=""
      [ngModelOptions]="{ standalone: true }"
      [formArray]="array.control"
      [order]="order"
    />
    @for (item of array.control.controls; track item) {
      <div tuiAnimated class="control">
        <div>
          @if (spec.spec.type === 'object') {
            <form-object
              class="object"
              [class.object_open]="!!open.get(item)"
              [formGroup]="$any(item)"
              [spec]="$any(spec.spec)"
              [open]="!!open.get(item)"
              (openChange)="open.set(item, $event)"
            >
              {{ item.value | mustache: $any(spec.spec).displayAs }}
              <button
                tuiIconButton
                type="button"
                class="remove"
                iconStart="@tui.trash"
                appearance="icon"
                size="m"
                title="Remove"
                (click.stop)="removeAt($index)"
              ></button>
            </form-object>
          } @else {
            <form-control
              class="array"
              tuiTextfieldSize="m"
              [formControl]="$any(item)"
              [spec]="$any(spec.spec)"
              (remove)="removeAt($index)"
            />
          }
        </div>
      </div>
    } @empty {
      <div class="placeholder">{{ 'The list is empty' | i18n }}</div>
    }
  `,
  styles: `
    @use '@taiga-ui/styles/utils' as taiga;

    :host {
      display: block;
      margin: 2rem 0;
    }

    .label {
      display: flex;
      align-items: center;
      font: var(--tui-typography-heading-h6);
    }

    .add {
      font-size: 1rem;
      padding: 0 1rem;
      margin-left: auto;
    }

    .object {
      display: block;
      position: relative;

      &_open::after,
      &:last-child::after {
        opacity: 0;
      }

      &::after {
        @include taiga.transition(opacity);

        content: '';
        position: absolute;
        bottom: -0.5rem;
        height: 1px;
        left: 3rem;
        right: 1rem;
        background: var(--tui-background-neutral-1);
      }
    }

    .remove {
      margin: 0 0.375rem 0 auto;
      pointer-events: auto;

      &::before {
        font-size: 1rem;
      }
    }

    .control {
      display: grid;

      form-control {
        display: block;
        margin: 0.5rem 0;
      }

      > * {
        overflow: hidden;
      }

      &.tui-enter,
      &.tui-leave {
        animation-name: tuiFade, tuiCollapse;
      }
    }

    .placeholder {
      display: flex;
      align-items: center;
      justify-content: center;
      height: var(--tui-height-m);
      color: var(--tui-text-tertiary);
      border-radius: var(--tui-radius-m);
      border: 1px dashed var(--tui-background-neutral-1);
      margin: 0.5rem 0 0;
    }
  `,
  hostDirectives: [ControlDirective],
  imports: [
    ReactiveFormsModule,
    TuiIcon,
    TuiTooltip,
    TuiLink,
    TuiError,
    TuiButton,
    TuiInput,
    i18nPipe,
    HintPipe,
    MustachePipe,
    FormControlComponent,
    forwardRef(() => FormObjectComponent),
    TuiAnimated,
    FormsModule,
  ],
})
export class FormArrayComponent {
  @Input({ required: true })
  spec!: IST.ValueSpecList

  readonly order = ERRORS
  readonly array = inject(FormArrayName)
  readonly open = new Map<AbstractControl, boolean>()

  private warned = false
  private readonly formService = inject(FormService)
  private readonly destroyRef = inject(DestroyRef)
  private readonly cdr = inject(ChangeDetectorRef)
  private readonly dialog = inject(DialogService)

  get canAdd(): boolean {
    return (
      !this.spec.disabled &&
      (this.spec.maxLength == null ||
        this.array.control.length < this.spec.maxLength)
    )
  }

  add() {
    if (!this.warned && this.spec.warning) {
      this.dialog
        .openConfirm({
          label: 'Warning',
          size: 's',
          data: {
            content: this.spec.warning as i18nKey,
            yes: 'Ok',
            no: 'Cancel',
          },
        })
        .pipe(filter(Boolean), takeUntilDestroyed(this.destroyRef))
        .subscribe(() => {
          this.addItem()
        })
    } else {
      this.addItem()
    }

    this.warned = true
  }

  removeAt(index: number) {
    this.removeItem(index)
  }

  private removeItem(index: number) {
    this.open.delete(this.array.control.at(index))
    this.array.control.removeAt(index)
  }

  private addItem() {
    this.array.control.insert(0, this.formService.getListItem(this.spec))
    this.open.set(this.array.control.at(0), true)
    this.cdr.markForCheck()
  }
}
