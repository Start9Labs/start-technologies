import { Component, forwardRef, inject, Input, OnChanges } from '@angular/core'
import {
  ControlContainer,
  FormGroupName,
  ReactiveFormsModule,
} from '@angular/forms'
import { IST } from '@start9labs/start-core'
import { tuiProvide, TuiValueChanges } from '@taiga-ui/cdk'
import { TuiElasticContainer } from '@taiga-ui/layout'
import { FormService } from 'src/app/services/form.service'
import { FormControlComponent } from './control.component'
import { FormGroupComponent } from './group.component'

@Component({
  selector: 'form-union',
  template: `
    <form-control
      [spec]="selectSpec"
      formControlName="selection"
      (tuiValueChanges)="onUnion($event)"
    />
    <tui-elastic-container class="g-form-group" formGroupName="value">
      <form-group
        class="group"
        [spec]="(union && spec.variants[union]?.spec) || {}"
      />
    </tui-elastic-container>
  `,
  styles: `
    :host {
      display: block;
    }

    .group {
      display: flow-root;
    }

    .group:not(:empty) {
      margin-block-start: 1rem;
    }
  `,
  viewProviders: [tuiProvide(ControlContainer, FormGroupName)],
  imports: [
    ReactiveFormsModule,
    TuiValueChanges,
    TuiElasticContainer,
    FormControlComponent,
    forwardRef(() => FormGroupComponent),
  ],
})
export class FormUnionComponent implements OnChanges {
  @Input({ required: true })
  spec!: IST.ValueSpecUnion

  selectSpec!: IST.ValueSpecSelect

  // List rows share specs; variant memory belongs to each union instance.
  private readonly others: Record<string, any> = {}

  private readonly form = inject(FormGroupName)
  private readonly formService = inject(FormService)

  get union(): string {
    return this.form.value.selection
  }

  onUnion(union: string) {
    this.others[this.union] = this.form.control.controls['value']?.value
    this.form.control.setControl(
      'value',
      this.formService.getFormGroup(
        union ? this.spec.variants[union]?.spec || {} : {},
        [],
        this.others[union],
      ),
      { emitEvent: false },
    )
  }

  ngOnChanges() {
    this.selectSpec = this.formService.getUnionSelectSpec(this.spec, this.union)
    if (this.union) this.onUnion(this.union)
  }
}
