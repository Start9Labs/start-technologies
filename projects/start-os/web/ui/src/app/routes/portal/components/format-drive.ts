import { Component, computed, inject } from '@angular/core'
import { takeUntilDestroyed, toSignal } from '@angular/core/rxjs-interop'
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
  DialogService,
  i18nPipe,
  TaskService,
} from '@start9labs/shared'
import { T } from '@start9labs/start-core'
import { tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  tuiIconStart,
  tuiValidationErrorsProvider,
  TuiButton,
  TuiCell,
  TuiCheckbox,
  TuiDialogContext,
  TuiError,
  TuiNotification,
  TuiNotificationService,
  TuiTextfield,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiChevron, TuiDataListWrapper, TuiSelect } from '@taiga-ui/kit'
import { TuiForm, TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'
import { filter } from 'rxjs'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { DataModel } from 'src/app/services/patch-db/data-model'

type Disk = T.DiskInfo

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
            'Erases a drive and prepares it as a single Btrfs partition.' | i18n
          }}
        </p>
      </hgroup>
    </header>
    @let drives = disks();
    <form tuiForm="m" [formGroup]="form" (submit.prevent)="format()">
      <tui-textfield
        tuiChevron
        [stringify]="stringify"
        [tuiTextfieldCleaner]="false"
      >
        <label tuiLabel>{{ 'Drive' | i18n }}</label>
        @if (mobile) {
          <select tuiSelect formControlName="disk" [items]="drives"></select>
        } @else {
          <input tuiSelect formControlName="disk" />
        }
        @if (!mobile) {
          <tui-data-list-wrapper
            *tuiDropdown
            [items]="drives"
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
      @if (selectedDisk(); as disk) {
        <div tuiNotification appearance="negative" size="m">
          <span tuiTitle>
            {{ 'Everything on this drive will be permanently erased.' | i18n }}
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
          [disabled]="form.invalid || !selectedDisk()"
        >
          {{ 'Format' | i18n }}
        </button>
      </footer>
    </form>
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
    TuiNotification,
    TuiSelect,
    TuiTextfield,
    TuiTitle,
  ],
  providers: [
    tuiValidationErrorsProvider(() => {
      const i18n = inject(i18nPipe)
      return {
        required: () => i18n.transform('Required'),
        labelMaxBytes: () =>
          i18n.transform(
            'Must be 255 bytes or less (accented letters and symbols take more than one)',
          ),
        labelControlCharacters: () =>
          i18n.transform('Must not contain control characters'),
      }
    }),
  ],
})
export class FormatDriveDialog {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly i18n = inject(i18nPipe)
  private readonly attached$ =
    inject<PatchDB<DataModel>>(PatchDB).watch$('disks')
  private readonly attached = toSignal(this.attached$, { initialValue: [] })

  protected readonly context = injectContext<TuiDialogContext<boolean>>()
  protected readonly mobile = inject(WA_IS_MOBILE)
  protected readonly disks = computed(() =>
    this.attached().filter(disk => !/-part\d+$/.test(disk.stablePath)),
  )
  protected readonly form = inject(NonNullableFormBuilder).group({
    disk: [null as Disk | null, Validators.required],
    label: ['StartOS Backups', labelValidator],
    acknowledged: [false, Validators.requiredTrue],
  })
  private readonly selection = toSignal(this.form.controls.disk.valueChanges, {
    initialValue: null,
  })
  protected readonly selectedDisk = computed(() =>
    this.disks().find(
      disk => disk.logicalname === this.selection()?.logicalname,
    ),
  )
  private readonly removedSelection = this.attached$
    .pipe(takeUntilDestroyed())
    .subscribe(disks => {
      const selected = this.form.controls.disk.value
      if (
        selected &&
        !disks.some(disk => disk.logicalname === selected.logicalname)
      ) {
        this.form.controls.disk.reset()
        this.form.controls.acknowledged.reset()
      }
    })

  protected readonly driveName = (drive: Disk): string =>
    [drive.vendor, drive.model].filter(Boolean).join(' ') ||
    this.i18n.transform('Unknown Drive')

  protected readonly stringify = (drive: Disk | null) =>
    drive
      ? `${this.driveName(drive)} · ${convertBytes(drive.capacity)} · ${drive.logicalname}`
      : ''

  protected hasBackups(partition: T.PartitionInfo): boolean {
    return partition.legacyBackup || Object.keys(partition.startOs).length > 0
  }

  protected format() {
    tuiMarkControlAsTouchedAndValidate(this.form)
    if (this.form.invalid) return
    const { label } = this.form.getRawValue()
    const disk = this.selectedDisk()

    if (!disk) return

    this.tasks.run(async () => {
      await this.api.rpc.disk.format({ disk: disk.logicalname, label })
      this.context.completeWith(true)
    }, 'Formatting drive')
  }
}

const FORMAT_DRIVE = new PolymorpheusComponent(FormatDriveDialog)

@Component({
  selector: 'button[formatDrive]',
  template: `
    {{ 'Format a drive' | i18n }}
  `,
  hostDirectives: [{ directive: TuiButton, inputs: ['size'] }],
  host: {
    tuiButton: '',
    type: 'button',
    '(click)': 'format()',
  },
  imports: [i18nPipe],
})
export class FormatDrive {
  private readonly dialogs = inject(DialogService)
  private readonly alerts = inject(TuiNotificationService)
  private readonly i18n = inject(i18nPipe)

  protected readonly icon = tuiIconStart('@tui.hard-drive')

  protected format() {
    this.dialogs
      .openComponent<boolean>(FORMAT_DRIVE, { size: 'm' })
      .pipe(filter(Boolean))
      .subscribe(() => {
        this.alerts
          .open(this.i18n.transform('Drive formatted'), {
            appearance: 'positive',
          })
          .subscribe()
      })
  }
}
