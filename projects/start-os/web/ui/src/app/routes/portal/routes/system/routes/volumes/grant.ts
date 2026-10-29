import { Component, inject, input, linkedSignal, signal } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { i18nPipe, TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiRadioList } from '@taiga-ui/kit'

import { ApiService } from 'src/app/services/api/embassy-api.service'

const permission = (grant: T.SharedVolumeGrant | undefined) =>
  !grant ? 'Off' : grant.readonly ? 'Read' : 'Read/write'

@Component({
  selector: 'volume-grant',
  template: `
    <tui-radio-list
      size="s"
      [items]="options"
      [itemContent]="content"
      [attr.aria-label]="label()"
      [disabled]="disabled() || pending()"
      [ngModel]="value()"
      (ngModelChange)="save($event)"
    />
    <ng-template #content let-option>{{ option | i18n }}</ng-template>
  `,
  imports: [FormsModule, i18nPipe, TuiRadioList],
})
export class VolumeGrant {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)

  protected readonly options = ['Off', 'Read', 'Read/write'] as const
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

  protected async save(value: (typeof this.options)[number]) {
    this.value.set(value)
    this.pending.set(true)
    const saved = await this.tasks.run(
      () =>
        this.api.rpc.volumes.setGrant({
          id: this.volumeId(),
          packageId: this.packageId(),
          grant: value === 'Off' ? null : { readonly: value === 'Read' },
        }),
      'Saving',
    )
    if (!saved) this.value.set(permission(this.grant()))
    this.pending.set(false)
  }
}
