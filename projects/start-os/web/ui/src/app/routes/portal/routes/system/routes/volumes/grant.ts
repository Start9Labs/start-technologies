import {
  Component,
  computed,
  inject,
  input,
  linkedSignal,
  signal,
} from '@angular/core'
import { i18nPipe, TaskService } from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { TuiIcon } from '@taiga-ui/core'
import { TuiSegmented } from '@taiga-ui/kit'

import { ApiService } from 'src/app/services/api/embassy-api.service'

const permission = (grant: T.SharedVolumeGrant | undefined) =>
  !grant ? 'Off' : grant.readonly ? 'Read' : 'Read/write'

@Component({
  selector: 'volume-grant',
  template: `
    <tui-segmented
      size="s"
      role="group"
      [attr.aria-label]="label()"
      [activeItemIndex]="activeIndex()"
    >
      @for (option of options; track option.label) {
        <button
          type="button"
          [attr.aria-pressed]="value() === option.label"
          [disabled]="disabled() || pending()"
          (click)="save(option.label)"
        >
          <tui-icon [icon]="option.icon" />
          {{ option.label | i18n }}
        </button>
      }
    </tui-segmented>
  `,
  styles: `
    :host {
      display: inline-flex;
    }
  `,
  imports: [i18nPipe, TuiIcon, TuiSegmented],
})
export class VolumeGrant {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)

  protected readonly options = [
    { label: 'Off', icon: '@tui.x' },
    { label: 'Read', icon: '@tui.eye' },
    { label: 'Read/write', icon: '@tui.pencil' },
  ] as const
  protected readonly pending = signal(false)
  protected readonly value = linkedSignal({
    source: () => ({
      id: this.volumeId(),
      packageId: this.packageId(),
      grant: this.grant(),
    }),
    computation: ({ grant }) => permission(grant),
  })

  protected readonly activeIndex = computed(() =>
    this.options.findIndex(option => option.label === this.value()),
  )

  public readonly volumeId = input.required<string>()
  public readonly packageId = input.required<string>()
  public readonly label = input.required<string>()
  public readonly grant = input<T.SharedVolumeGrant>()
  public readonly disabled = input(false)

  protected async save(value: (typeof this.options)[number]['label']) {
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
