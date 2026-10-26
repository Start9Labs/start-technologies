import { Component, inject, input, linkedSignal, output } from '@angular/core'
import { takeUntilDestroyed, toObservable } from '@angular/core/rxjs-interop'
import {
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { i18nPipe } from '@start9labs/shared'
import { TuiButton, TuiDataList, TuiInput } from '@taiga-ui/core'
import { TuiChevron, TuiInputNumber, TuiSelect } from '@taiga-ui/kit'
import { map, merge, switchMap } from 'rxjs'

import {
  BACKUP_RETENTION_INTERVALS,
  BackupRetentionInterval,
  backupRetentionIntervalLabel,
  BackupRetentionRuleValue,
  formatBackupRetentionRule,
  retentionPeriodLabel,
} from './scheduled-utils'

@Component({
  selector: 'backup-retention-rules',
  template: `
    <div class="retention-rules">
      @for (ruleForm of ruleForms(); track ruleForm; let index = $index) {
        <div class="retention-rule" [formGroup]="ruleForm">
          @let rule = rules()[index];
          @if (rule && rule.interval === 'custom') {
            <span class="custom-rule">{{ customRule(rule) }}</span>
          } @else {
            <span>{{ 'Keep one backup every' | i18n }}</span>
            <tui-textfield
              tuiChevron
              [stringify]="stringifyInterval"
              [tuiTextfieldCleaner]="false"
            >
              <label tuiLabel>{{ 'Frequency' | i18n }}</label>
              <input tuiSelect formControlName="interval" />
              <tui-data-list *tuiDropdown>
                @for (interval of intervals; track interval) {
                  <button tuiOption [value]="interval">
                    {{ stringifyInterval(interval) }}
                  </button>
                }
              </tui-data-list>
            </tui-textfield>
            <span>{{ 'for' | i18n }}</span>
            <tui-textfield class="duration-field">
              <label tuiLabel>{{ 'Duration' | i18n }}</label>
              <input
                tuiInputNumber
                formControlName="duration"
                [min]="1"
                [max]="365"
              />
            </tui-textfield>
            <span>{{ period(ruleForm.getRawValue()) | i18n }}</span>
          }
          <button
            tuiButton
            type="button"
            size="xs"
            appearance="flat-destructive"
            (click)="removeRequested.emit(index)"
          >
            {{ 'Remove' | i18n }}
          </button>
        </div>
      }
      <button
        tuiIconButton
        type="button"
        class="add-retention-rule"
        size="s"
        appearance="primary"
        iconStart="@tui.plus"
        [attr.aria-label]="'Add' | i18n"
        (click)="addRequested.emit()"
      >
        {{ 'Add' | i18n }}
      </button>
    </div>
  `,
  styles: `
    .retention-rules {
      display: grid;
      justify-items: stretch;
      gap: 0.75rem;
      inline-size: 100%;
      min-inline-size: 0;
    }

    .retention-rule {
      display: grid;
      grid-template-columns:
        auto minmax(9rem, 1fr) auto minmax(10rem, 0.75fr)
        auto auto;
      gap: 0.5rem;
      align-items: center;
      inline-size: 100%;
      min-inline-size: 0;
    }

    .custom-rule {
      grid-column: 1 / -2;
    }

    .duration-field {
      min-inline-size: 10rem;
    }

    .add-retention-rule {
      justify-self: end;
    }

    :host-context(tui-root._mobile) .retention-rule {
      grid-template-columns: 1fr;
    }

    :host-context(tui-root._mobile) .custom-rule {
      grid-column: auto;
    }
  `,
  imports: [
    ReactiveFormsModule,
    TuiButton,
    TuiChevron,
    TuiDataList,
    TuiInput,
    TuiInputNumber,
    TuiSelect,
    i18nPipe,
  ],
})
export class BackupRetentionRules {
  private readonly formBuilder = inject(NonNullableFormBuilder)
  private readonly i18n = inject(i18nPipe)

  readonly rules = input.required<readonly BackupRetentionRuleValue[]>()
  readonly ruleChange = output<{
    index: number
    value: BackupRetentionRuleValue
  }>()
  readonly addRequested = output<void>()
  readonly removeRequested = output<number>()

  protected readonly intervals = BACKUP_RETENTION_INTERVALS
  protected readonly ruleForms = linkedSignal<
    readonly BackupRetentionRuleValue[],
    ReturnType<BackupRetentionRules['createRuleForm']>[]
  >({
    source: this.rules,
    computation: (rules, previous) =>
      previous &&
      rules.length === previous.source.length &&
      rules.every((rule, index) => rule === previous.source[index])
        ? previous.value
        : rules.map(rule => this.createRuleForm(rule)),
  })
  protected readonly stringifyInterval = (
    interval: BackupRetentionRuleValue['interval'],
  ) => this.i18n.transform(backupRetentionIntervalLabel(interval))
  private readonly ruleChangesSubscription = toObservable(this.ruleForms)
    .pipe(
      switchMap(forms =>
        merge(
          ...forms.map((form, index) =>
            form.valueChanges.pipe(map(() => ({ form, index }))),
          ),
        ),
      ),
      takeUntilDestroyed(),
    )
    .subscribe(({ form, index }) => {
      this.ruleChange.emit({
        index,
        value: { ...this.rules()[index], ...form.getRawValue() },
      })
    })

  protected customRule(rule: BackupRetentionRuleValue) {
    return formatBackupRetentionRule(rule, label => this.i18n.transform(label))
  }

  protected period(rule: BackupRetentionRuleValue) {
    return rule.interval === 'custom'
      ? 'hours'
      : retentionPeriodLabel(rule.interval, rule.duration)
  }

  private createRuleForm(rule: BackupRetentionRuleValue) {
    return this.formBuilder.group({
      interval: [rule.interval],
      duration: [
        rule.duration,
        [Validators.required, Validators.min(1), Validators.max(365)],
      ],
    })
  }
}
