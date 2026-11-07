import { Component, computed, inject, signal } from '@angular/core'
import { takeUntilDestroyed, toSignal } from '@angular/core/rxjs-interop'
import {
  FormsModule,
  NonNullableFormBuilder,
  ReactiveFormsModule,
  Validators,
} from '@angular/forms'
import { ConvertBytesPipe, i18nPipe, TaskService } from '@start9labs/shared'
import { WA_IS_MOBILE } from '@ng-web-apis/platform'
import { TuiAutoFocus, tuiMarkControlAsTouchedAndValidate } from '@taiga-ui/cdk'
import {
  tuiValidationErrorsProvider,
  TuiButton,
  TuiDataList,
  TuiDialogContext,
  TuiError,
  TuiIcon,
  TuiInput,
  TuiTitle,
} from '@taiga-ui/core'
import { TuiChevron, TuiPassword, TuiRadioList, TuiSelect } from '@taiga-ui/kit'
import { TuiForm, TuiHeader } from '@taiga-ui/layout'
import { injectContext, PolymorpheusComponent } from '@taiga-ui/polymorpheus'
import { PatchDB } from 'patch-db-client'

import { DataModel } from 'src/app/services/patch-db/data-model'
import { ApiService } from 'src/app/services/api/embassy-api.service'
import { getPartitionName } from 'src/app/utils/get-partition-name'

@Component({
  template: `
    <form tuiForm="m" [formGroup]="form" (submit.prevent)="save()">
      <header tuiHeader>
        <h2 tuiTitle [id]="context.id">{{ 'New volume' | i18n }}</h2>
      </header>
      <tui-textfield>
        <label tuiLabel>{{ 'Name' | i18n }}</label>
        <input tuiInput formControlName="name" [tuiAutoFocus]="!isMobile" />
      </tui-textfield>
      <tui-error formControlName="name" />
      <fieldset>
        <legend>{{ 'Source' | i18n }}</legend>
        <tui-radio-list
          [items]="sources"
          [itemContent]="sourceContent"
          [ngModelOptions]="{ standalone: true }"
          [(ngModel)]="source"
        />
        <ng-template #sourceContent let-item>{{ item | i18n }}</ng-template>
      </fieldset>
      @if (source() === 'External drive') {
        <tui-textfield
          tuiChevron
          [stringify]="driveLabel"
          [tuiTextfieldCleaner]="false"
        >
          <label tuiLabel>{{ 'Drive' | i18n }}</label>
          <input tuiSelect formControlName="logicalname" required />
          <tui-data-list *tuiDropdown>
            @for (drive of drives(); track drive.logicalname) {
              <button tuiOption [value]="drive.logicalname">
                {{ partitionName(drive) }} ·
                {{ drive.capacity | convertBytes }}
              </button>
            } @empty {
              {{ 'No drives detected' | i18n }}
            }
          </tui-data-list>
        </tui-textfield>
        <tui-error formControlName="logicalname" />
        <tui-textfield>
          <label tuiLabel>{{ 'Path' | i18n }}</label>
          <input
            tuiInput
            formControlName="path"
            placeholder="/"
            autocapitalize="off"
          />
        </tui-textfield>
        <p>
          {{
            'Relative to the selected drive. Missing folders will be created.'
              | i18n
          }}
        </p>
      }
      @if (source() === 'CIFS share') {
        <tui-textfield>
          <label tuiLabel>{{ 'Hostname' | i18n }}</label>
          <input
            tuiInput
            formControlName="hostname"
            required
            autocapitalize="off"
          />
        </tui-textfield>
        <tui-error formControlName="hostname" />
        <tui-textfield>
          <label tuiLabel>{{ 'Path' | i18n }}</label>
          <input
            tuiInput
            formControlName="path"
            required
            autocapitalize="off"
          />
        </tui-textfield>
        <tui-error formControlName="path" />
        <tui-textfield>
          <label tuiLabel>{{ 'Username' | i18n }}</label>
          <input
            tuiInput
            formControlName="username"
            autocomplete="off"
            autocapitalize="off"
          />
        </tui-textfield>
        <tui-textfield>
          <label tuiLabel>{{ 'Password' | i18n }}</label>
          <input
            tuiInput
            type="password"
            formControlName="password"
            autocomplete="new-password"
          />
          <tui-icon tuiPassword />
        </tui-textfield>
      }
      <p>
        {{
          'The source cannot be changed after creation. Passwords remain private.'
            | i18n
        }}
      </p>
      <footer>
        <button
          tuiButton
          type="button"
          appearance="secondary"
          (click)="context.$implicit.complete()"
        >
          {{ 'Cancel' | i18n }}
        </button>
        <button tuiButton>{{ 'Save' | i18n }}</button>
      </footer>
    </form>
  `,
  providers: [
    tuiValidationErrorsProvider(() => {
      const i18n = inject(i18nPipe)
      return { required: () => i18n.transform('Required') }
    }),
  ],
  imports: [
    FormsModule,
    ReactiveFormsModule,
    i18nPipe,
    ConvertBytesPipe,
    TuiButton,
    TuiChevron,
    TuiDataList,
    TuiSelect,
    TuiAutoFocus,
    TuiError,
    TuiIcon,
    TuiInput,
    TuiTitle,
    TuiPassword,
    TuiRadioList,
    TuiForm,
    TuiHeader,
  ],
})
export class CreateVolume {
  private readonly api = inject(ApiService)
  private readonly tasks = inject(TaskService)
  private readonly attached$ =
    inject<PatchDB<DataModel>>(PatchDB).watch$('disks')
  private readonly disks = toSignal(this.attached$, { initialValue: [] })

  protected readonly drives = computed(() =>
    this.disks().flatMap(disk => disk.partitions),
  )
  protected readonly isMobile = inject(WA_IS_MOBILE)
  protected readonly context = injectContext<TuiDialogContext<void>>()
  protected readonly sources = [
    'Local folder',
    'External drive',
    'CIFS share',
  ] as const
  protected readonly source =
    signal<(typeof this.sources)[number]>('Local folder')
  protected readonly form = inject(NonNullableFormBuilder).group({
    name: ['', Validators.required],
    logicalname: [''],
    hostname: [''],
    path: [''],
    username: [''],
    password: [''],
  })

  private readonly removedSelection = this.attached$
    .pipe(takeUntilDestroyed())
    .subscribe(disks => {
      const logicalname = this.form.controls.logicalname.value
      if (
        logicalname &&
        !disks.some(disk =>
          disk.partitions.some(
            partition => partition.logicalname === logicalname,
          ),
        )
      ) {
        this.form.controls.logicalname.reset()
      }
    })

  protected readonly partitionName = getPartitionName
  protected readonly driveLabel = (logicalname: string) => {
    const partition = this.drives().find(
      drive => drive.logicalname === logicalname,
    )
    return partition ? getPartitionName(partition) : logicalname
  }

  protected async save() {
    tuiMarkControlAsTouchedAndValidate(this.form)
    if (this.form.invalid) return
    const { name, logicalname, hostname, path, username, password } =
      this.form.getRawValue()
    if (
      await this.tasks.run(
        () =>
          this.api.rpc.volumes.create({
            name,
            source:
              this.source() === 'Local folder'
                ? { kind: 'local' }
                : this.source() === 'External drive'
                  ? { kind: 'disk', logicalname, path }
                  : { kind: 'cifs', hostname, path, username, password },
          }),
        'Saving',
      )
    )
      this.context.$implicit.complete()
  }
}

export const CREATE_VOLUME = new PolymorpheusComponent(CreateVolume)
