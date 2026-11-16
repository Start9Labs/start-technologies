import {
  Component,
  computed,
  inject,
  input,
  linkedSignal,
  output,
  signal,
} from '@angular/core'
import { FormsModule } from '@angular/forms'
import { i18nPipe, TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiDataList, TuiInput } from '@taiga-ui/core'
import { TuiChevron, TuiSelect } from '@taiga-ui/kit'

import { ApiService } from 'src/app/services/api/embassy-api.service'

const GRANT_OPTIONS = [
  { label: 'No Access', icon: '@tui.x', grant: null },
  { label: 'Read', icon: '@tui.eye', grant: { readonly: true } },
  { label: 'Read/write', icon: '@tui.pencil', grant: { readonly: false } },
] as const

export type GrantLabel = (typeof GRANT_OPTIONS)[number]['label']

export const permission = (grant: T.SharedVolumeGrant | null | undefined) =>
  GRANT_OPTIONS.find(option => option.grant?.readonly === grant?.readonly)!
    .label

export const grantFor = (label: GrantLabel) =>
  GRANT_OPTIONS.find(option => option.label === label)!.grant

@Component({
  selector: 'grant-choice',
  host: { '[style.inline-size]': 'width' },
  template: `
    <tui-textfield
      tuiChevron
      tuiTextfieldSize="s"
      [iconStart]="icon()"
      [tuiTextfieldCleaner]="false"
      [stringify]="stringify"
    >
      <input
        tuiSelect
        [attr.aria-label]="label()"
        [disabled]="disabled()"
        [ngModel]="value()"
        (ngModelChange)="selected.emit($event)"
      />
      <tui-data-list *tuiDropdown>
        @for (option of options; track option.label) {
          <button tuiOption [iconStart]="option.icon" [value]="option.label">
            {{ option.label | i18n }}
          </button>
        }
      </tui-data-list>
    </tui-textfield>
  `,
  styles: `
    :host {
      display: block;
    }
  `,
  imports: [
    FormsModule,
    i18nPipe,
    TuiDataList,
    TuiInput,
    TuiSelect,
    TuiChevron,
  ],
})
export class GrantChoice {
  private readonly i18n = inject(i18nPipe)

  protected readonly options = GRANT_OPTIONS
  protected readonly icon = computed(
    () => this.options.find(option => option.label === this.value())!.icon,
  )
  protected readonly stringify = (label: GrantLabel) =>
    this.i18n.transform(label)
  protected readonly width = `calc(${Math.max(
    ...this.options.map(({ label }) => this.stringify(label).length),
  )}ch + 4rem)`

  public readonly label = input.required<string>()
  public readonly value = input.required<GrantLabel>()
  public readonly disabled = input(false)
  public readonly selected = output<GrantLabel>()
}

@Component({
  selector: 'volume-grant',
  template: `
    <grant-choice
      [label]="label()"
      [value]="value()"
      [disabled]="disabled() || pending()"
      (selected)="save($event)"
    />
  `,
  styles: `
    :host {
      display: inline-flex;
    }
  `,
  imports: [GrantChoice],
})
export class VolumeGrant {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)

  protected readonly pending = signal(false)
  protected readonly value = linkedSignal({
    source: () => ({
      id: this.volumeId(),
      packageId: this.packageId(),
      grant: this.grant(),
    }),
    computation: ({ grant }) => permission(grant),
  })

  public readonly volumeId = input.required<string>()
  public readonly packageId = input.required<string>()
  public readonly label = input.required<string>()
  public readonly grant = input<T.SharedVolumeGrant>()
  public readonly disabled = input(false)

  protected async save(value: GrantLabel) {
    this.value.set(value)
    this.pending.set(true)
    const saved = await this.tasks.run(
      () =>
        this.api.rpc.volumes.setGrant({
          id: this.volumeId(),
          packageId: this.packageId(),
          grant: grantFor(value),
        }),
      'Saving',
    )
    if (!saved) this.value.set(permission(this.grant()))
    this.pending.set(false)
  }
}
