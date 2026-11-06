import { Component, inject, signal } from '@angular/core'
import {
  AbstractControl,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  ValidationErrors,
  Validators,
} from '@angular/forms'
import { WA_IS_MOBILE } from '@ng-web-apis/platform'
import {
  convertBytes,
  ConvertBytesPipe,
  ErrorService,
  HttpError,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import {
  TUI_VALIDATION_ERRORS,
  TuiButton,
  TuiCell,
  TuiCheckbox,
  TuiDialogContext,
  TuiError,
  TuiLoader,
  TuiNotification,
  TuiTextfield,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiChevron, TuiDataListWrapper, TuiSelect } from '@taiga-ui/kit'
import { TuiForm, TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { Result } from 'src/app/services/api/api.types'
import { ApiService } from 'src/app/services/api/embassy-api.service'

type Disk = Result<'disk.list'>[number]

const MAX_LABEL_BYTES = 255

function labelValidator(control: AbstractControl): ValidationErrors | null {
  const label: string = control.value || ''

  if (!label) return { required: true }
  if (new TextEncoder().encode(label).length > MAX_LABEL_BYTES) {
    return { labelMaxBytes: true }
  }
  if (/\p{Cc}/u.test(label)) return { labelControlCharacters: true }

  return null
}

@Component({
  template: `
    <header tuiHeader>
      <hgroup tuiTitle>
        <h2 [id]="context.id">{{ 'Format a drive' | i18n }}</h2>
        <p>
          {{
            'Erases a drive and prepares it as a single Btrfs partition for backups.'
              | i18n
          }}
        </p>
      </hgroup>
    </header>
    @if (disks(); as disks) {
      <form tuiForm="m" [formGroup]="form" (submit.prevent)="format()">
        <tui-textfield
          tuiChevron
          [stringify]="stringify"
          [tuiTextfieldCleaner]="false"
        >
          <label tuiLabel>{{ 'Drive' | i18n }}</label>
          @if (mobile) {
            <select tuiSelect formControlName="disk" [items]="disks"></select>
          } @else {
            <input tuiSelect formControlName="disk" />
          }
          @if (!mobile) {
            <tui-data-list-wrapper
              *tuiDropdown
              [items]="disks"
              [itemContent]="driveContent"
            />
          }
        </tui-textfield>
        <ng-template #driveContent let-drive>
          <span tuiTitle>
            {{ driveName(drive) }}
            <span tuiSubtitle>
              {{ drive.capacity | convertBytes }} · {{ drive.logicalname }}
            </span>
          </span>
        </ng-template>
        @if (form.controls.disk.value; as disk) {
          <div tuiNotification appearance="negative" size="m">
            <span tuiTitle>
              {{
                'Everything on this drive will be permanently erased.' | i18n
              }}
              <span tuiSubtitle>
                @for (p of disk.partitions; track p.logicalname) {
                  <div>
                    {{ p.label || p.logicalname }},
                    {{ p.capacity | convertBytes }}
                    @if (hasBackups(p)) {
                      &mdash; {{ 'contains StartOS backups' | i18n }}
                    }
                  </div>
                } @empty {
                  {{ 'No readable partitions' | i18n }}
                }
              </span>
            </span>
          </div>
        }
        <tui-textfield>
          <label tuiLabel>{{ 'Label' | i18n }}</label>
          <input tuiInput formControlName="label" maxlength="255" />
        </tui-textfield>
        <tui-error formControlName="label" />
        <label tuiCell>
          <input tuiCheckbox type="checkbox" formControlName="acknowledged" />
          <span tuiTitle>
            {{ 'I understand this drive will be erased' | i18n }}
          </span>
        </label>
        <footer>
          <button
            tuiButton
            appearance="flat"
            type="button"
            (click)="context.$implicit.complete()"
          >
            {{ 'Cancel' | i18n }}
          </button>
          <button
            tuiButton
            appearance="primary-destructive"
            [disabled]="form.invalid"
          >
            {{ 'Format' | i18n }}
          </button>
        </footer>
      </form>
    } @else {
      <tui-loader size="l" [style.height.rem]="10" />
    }
  `,
  imports: [
    ReactiveFormsModule,
    ConvertBytesPipe,
    i18nPipe,
    TuiButton,
    TuiCell,
    TuiCheckbox,
    TuiChevron,
    TuiDataListWrapper,
    TuiError,
    TuiForm,
    TuiHeader,
    TuiLoader,
    TuiNotification,
    TuiSelect,
    TuiTextfield,
    TuiTitle,
  ],
  providers: [
    {
      provide: TUI_VALIDATION_ERRORS,
      useFactory: () => {
        const i18n = inject(i18nPipe)
        return {
          required: i18n.transform('Required'),
          labelMaxBytes: i18n.transform(
            'Must be 255 bytes or less (accented letters and symbols take more than one)',
          ),
          labelControlCharacters: i18n.transform(
            'Must not contain control characters',
          ),
        }
      },
    },
  ],
})
export class FormatDriveDialog {
  private readonly api = inject(ApiService)
  private readonly errorService = inject(ErrorService)
  private readonly tasks = inject(TaskService)
  private readonly i18n = inject(i18nPipe)

  protected readonly context = injectContext<TuiDialogContext<boolean>>()
  protected readonly mobile = inject(WA_IS_MOBILE)
  protected readonly disks = signal<Disk[] | null>(null)
  protected readonly form = inject(NonNullableFormBuilder).group({
    disk: [null as Disk | null, Validators.required],
    label: ['StartOS Backups', labelValidator],
    acknowledged: [false, Validators.requiredTrue],
  })

  protected readonly driveName = (drive: Disk): string =>
    [drive.vendor, drive.model].filter(Boolean).join(' ') ||
    this.i18n.transform('Unknown Drive')

  protected readonly stringify = (drive: Disk | null) =>
    drive
      ? `${this.driveName(drive)} · ${convertBytes(drive.capacity)} · ${drive.logicalname}`
      : ''

  constructor() {
    this.load()
  }

  protected hasBackups(partition: T.PartitionInfo): boolean {
    return partition.legacyBackup || Object.keys(partition.startOs).length > 0
  }

  protected format() {
    const { disk, label } = this.form.getRawValue()

    if (!disk) return

    this.tasks.run(async () => {
      await this.api.rpc.disk.format({ disk: disk.logicalname, label })
      this.context.completeWith(true)
    }, 'Formatting drive')
  }

  private async load() {
    try {
      const disks = await this.api.rpc.disk.list({})

      this.disks.set(disks.filter(d => !/-part\d+$/.test(d.stablePath)))
    } catch (e) {
      this.errorService.handleError(e as HttpError)
      this.context.$implicit.complete()
    }
  }
}

export const FORMAT_DRIVE = new PolymorpheusComponent(FormatDriveDialog)
